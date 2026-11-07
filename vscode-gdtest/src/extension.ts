import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';
import {
  GdtestEvent,
  Outcome,
  detailsOf,
  formatUs,
  outcomeOf,
  parseEvent,
  projectRelativePath,
  testTarget,
} from './results';

const INSTALL_DOCS_URL = 'https://github.com/q-rault/gdtest#install';

/** What other extensions, and this extension's tests, can use. */
export interface GdtestApi {
  runAll(): Promise<Map<string, Outcome>>;
}

export function activate(context: vscode.ExtensionContext): GdtestApi {
  const outputChannel = vscode.window.createOutputChannel('gdtest');
  const controller = vscode.tests.createTestController('gdtest', 'gdtest');
  context.subscriptions.push(controller, outputChannel);

  const gdtestExe = vscode.workspace.getConfiguration('gdtest').get<string>('executablePath', 'gdtest');
  checkCli(gdtestExe).then((found) => {
    if (!found) {
      vscode.window
        .showWarningMessage('gdtest CLI not found. Install it to run tests.', 'Show Installation Guide')
        .then((choice) => {
          if (choice) vscode.env.openExternal(vscode.Uri.parse(INSTALL_DOCS_URL));
        });
    }
  });

  controller.refreshHandler = () => discoverTests(controller);
  const discovered = discoverTests(controller);

  const watcher = vscode.workspace.createFileSystemWatcher('**/test_*.gd');
  watcher.onDidCreate(() => discoverTests(controller));
  watcher.onDidDelete(() => discoverTests(controller));
  context.subscriptions.push(watcher);

  controller.createRunProfile(
    'Run',
    vscode.TestRunProfileKind.Run,
    (request, token) => runTests(request, token, controller, outputChannel).then(() => undefined),
    true,
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('gdtest.runAll', () => vscode.commands.executeCommand('testing.runAll')),
    vscode.commands.registerCommand('gdtest.runFile', () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || !editor.document.fileName.endsWith('.gd')) {
        vscode.window.showWarningMessage('gdtest: open a .gd test file first');
        return;
      }
      vscode.commands.executeCommand('testing.runCurrentFile');
    }),
  );

  return {
    runAll: async () => {
      await discovered;
      const source = new vscode.CancellationTokenSource();
      return runTests(new vscode.TestRunRequest(), source.token, controller, outputChannel);
    },
  };
}

export function deactivate(): void {}

async function discoverTests(controller: vscode.TestController): Promise<void> {
  if (!vscode.workspace.workspaceFolders) return;
  const testFiles = await vscode.workspace.findFiles('**/test_*.gd', '{**/node_modules/**,**/addons/**}');
  controller.items.replace(
    testFiles.map((uri) => controller.createTestItem(uri.toString(), path.basename(uri.fsPath), uri)),
  );
}

/** One run of the CLI. Resolves with the outcome of each test, by test item id. */
function runTests(
  request: vscode.TestRunRequest,
  token: vscode.CancellationToken,
  controller: vscode.TestController,
  outputChannel: vscode.OutputChannel,
): Promise<Map<string, Outcome>> {
  const config = vscode.workspace.getConfiguration('gdtest');
  const gdtestPath = config.get<string>('executablePath', 'gdtest');
  const godotExe = config.get<string>('godotExecutablePath', '');
  const testDir = config.get<string>('testDirectory', 'test/');
  const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? '';

  const run = controller.createTestRun(request);
  outputChannel.clear();
  outputChannel.show(true);

  const targets =
    request.include && request.include.length > 0
      ? request.include.map((item) => {
          const relative = path.relative(workspaceRoot, item.uri!.fsPath);
          return testTarget(relative, item.parent ? item.label : undefined);
        })
      : [testDir];
  const godotFlag = godotExe ? ['--godot-exe', godotExe] : [];
  const args = ['--reporter', 'json', ...godotFlag, ...targets];
  outputChannel.appendLine(`$ ${gdtestPath} ${args.join(' ')}\n`);

  const outcomes = new Map<string, Outcome>();
  const running = new Set<vscode.TestItem>();

  return new Promise((resolve) => {
    const finish = () => {
      // A test the CLI started but never reported on did not finish.
      for (const item of running) {
        run.errored(item, new vscode.TestMessage('gdtest ended before this test finished'));
        outcomes.set(item.id, 'errored');
      }
      run.end();
      resolve(outcomes);
    };

    if (token.isCancellationRequested) {
      finish();
      return;
    }

    const proc = cp.spawn(gdtestPath, args, { cwd: workspaceRoot });
    let buffer = '';

    proc.stdout.on('data', (chunk: Buffer) => {
      buffer += chunk.toString();
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';
      for (const line of lines) {
        const event = parseEvent(line);
        if (event) {
          applyEvent(event, controller, run, workspaceRoot, outputChannel, running, outcomes);
        }
      }
    });
    proc.stderr.on('data', (chunk: Buffer) => outputChannel.append(chunk.toString()));
    token.onCancellationRequested(() => proc.kill());

    // Without this, a missing executable would leave the run spinning forever.
    proc.on('error', (error) => {
      outputChannel.appendLine(`Could not run ${gdtestPath}: ${error.message}`);
      finish();
    });
    proc.on('close', finish);
  });
}

function applyEvent(
  event: GdtestEvent,
  controller: vscode.TestController,
  run: vscode.TestRun,
  workspaceRoot: string,
  outputChannel: vscode.OutputChannel,
  running: Set<vscode.TestItem>,
  outcomes: Map<string, Outcome>,
): void {
  if (event.kind === 'summary') {
    outputChannel.appendLine(
      `\n${event.pass} passed, ${event.fail} failed, ${event.error} errored, ` +
        `${event.pending} pending, ${event.skipped} skipped`,
    );
    return;
  }
  if (!event.file || !event.name) return;

  const item = testItemFor(event.file, event.name, controller, workspaceRoot);
  if (event.kind === 'test_start') {
    running.add(item);
    run.started(item);
    return;
  }

  const outcome = outcomeOf(event);
  if (!outcome) return;
  running.delete(item);
  outcomes.set(item.id, outcome);

  const duration = event.us !== undefined ? event.us / 1000 : undefined;
  const details = detailsOf(event);
  const message = new vscode.TestMessage(details.join('\n'));
  switch (outcome) {
    case 'passed':
      outputChannel.appendLine(`  ✓ ${event.name} (${formatUs(event.us ?? 0)})`);
      run.passed(item, duration);
      break;
    case 'failed':
      outputChannel.appendLine(`  ✗ ${event.name}`);
      run.failed(item, message, duration);
      break;
    case 'errored':
      outputChannel.appendLine(`  ! ${event.name}`);
      run.errored(item, message, duration);
      break;
    case 'skipped':
      outputChannel.appendLine(`  - ${event.name} (skipped)`);
      run.skipped(item);
      break;
  }
  details.forEach((line) => outputChannel.appendLine(`      → ${line}`));
}

/** The item for a test, creating it and its file's item as results come in. */
function testItemFor(
  file: string,
  name: string,
  controller: vscode.TestController,
  workspaceRoot: string,
): vscode.TestItem {
  const uri = vscode.Uri.file(path.join(workspaceRoot, projectRelativePath(file)));
  let fileItem = controller.items.get(uri.toString());
  if (!fileItem) {
    fileItem = controller.createTestItem(uri.toString(), path.basename(uri.fsPath), uri);
    controller.items.add(fileItem);
  }
  const id = `${uri.toString()}::${name}`;
  let item = fileItem.children.get(id);
  if (!item) {
    item = controller.createTestItem(id, name, uri);
    fileItem.children.add(item);
  }
  return item;
}

function checkCli(gdtestPath: string): Promise<boolean> {
  return new Promise((resolve) => {
    cp.execFile(gdtestPath, ['--version'], { timeout: 3000 }, (err) => resolve(!err));
  });
}
