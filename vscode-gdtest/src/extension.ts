import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';

const GDTEST_OUT_PREFIX = 'GDTEST_OUT:';

interface GdtestEvent {
  event: string;
  name: string;
  us?: number;
  failures?: string[];
  message?: string;
}

export function activate(context: vscode.ExtensionContext): void {
  const outputChannel = vscode.window.createOutputChannel('gdtest');

  const controller = vscode.tests.createTestController('gdtest', 'gdtest');
  context.subscriptions.push(controller);
  context.subscriptions.push(outputChannel);

  controller.refreshHandler = () => discoverTests(controller);
  discoverTests(controller);

  // Watch for new test files
  const watcher = vscode.workspace.createFileSystemWatcher('**/test_*.gd');
  watcher.onDidCreate(() => discoverTests(controller));
  watcher.onDidDelete(() => discoverTests(controller));
  context.subscriptions.push(watcher);

  // Run handler: streams gdtest output and maps to test items
  controller.createRunProfile(
    'Run',
    vscode.TestRunProfileKind.Run,
    (request, token) => runTests(request, token, controller, outputChannel),
    true,
  );

  // Commands
  context.subscriptions.push(
    vscode.commands.registerCommand('gdtest.runAll', () => {
      const req = new vscode.TestRunRequest();
      controller.createRunProfile(
        'Run',
        vscode.TestRunProfileKind.Run,
        (r, t) => runTests(r, t, controller, outputChannel),
        true,
      );
      vscode.commands.executeCommand('testing.runAll');
    }),
    vscode.commands.registerCommand('gdtest.runFile', () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || !editor.document.fileName.endsWith('.gd')) {
        vscode.window.showWarningMessage('gdtest: open a .gd test file first');
        return;
      }
      vscode.commands.executeCommand('testing.runCurrentFile');
    }),
  );
}

async function discoverTests(controller: vscode.TestController): Promise<void> {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders) return;

  const testFiles = await vscode.workspace.findFiles('**/test_*.gd', '**/node_modules/**');
  controller.items.replace(
    testFiles.map((uri) => {
      const item = controller.createTestItem(uri.toString(), path.basename(uri.fsPath), uri);
      item.canResolveChildren = false;
      return item;
    }),
  );
}

async function runTests(
  request: vscode.TestRunRequest,
  token: vscode.CancellationToken,
  controller: vscode.TestController,
  outputChannel: vscode.OutputChannel,
): Promise<void> {
  const config = vscode.workspace.getConfiguration('gdtest');
  const gdtestPath = config.get<string>('executablePath', 'gdtest');
  const testDir = config.get<string>('testDirectory', 'test/');
  const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? '';

  const run = controller.createTestRun(request);
  outputChannel.clear();
  outputChannel.show(true);

  // Determine which files to run
  const args: string[] =
    request.include && request.include.length > 0
      ? request.include.map((item) => path.relative(workspaceRoot, item.uri!.fsPath))
      : [testDir];

  outputChannel.appendLine(`$ ${gdtestPath} ${args.join(' ')}\n`);

  // Mark all requested items as running
  const itemMap = new Map<string, vscode.TestItem>();
  controller.items.forEach((item) => {
    if (!request.include || request.include.includes(item)) {
      run.started(item);
      itemMap.set(path.basename(item.uri!.fsPath), item);
    }
  });

  await new Promise<void>((resolve) => {
    if (token.isCancellationRequested) {
      run.end();
      resolve();
      return;
    }

    const proc = cp.spawn(gdtestPath, args, { cwd: workspaceRoot });
    let buffer = '';
    let currentFileItem: vscode.TestItem | undefined;

    proc.stdout.on('data', (chunk: Buffer) => {
      const text = chunk.toString();
      buffer += text;
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';
      for (const line of lines) {
        if (line.startsWith(GDTEST_OUT_PREFIX)) {
          try {
            const ev: GdtestEvent = JSON.parse(line.slice(GDTEST_OUT_PREFIX.length));
            processEvent(ev, run, itemMap, outputChannel, (item) => {
              currentFileItem = item;
            });
          } catch {
            // malformed JSON — ignore
          }
        }
      }
    });

    proc.stderr.on('data', (chunk: Buffer) => {
      outputChannel.appendLine(chunk.toString());
    });

    token.onCancellationRequested(() => proc.kill());

    proc.on('close', () => {
      run.end();
      resolve();
    });
  });
}

function processEvent(
  ev: GdtestEvent,
  run: vscode.TestRun,
  itemMap: Map<string, vscode.TestItem>,
  outputChannel: vscode.OutputChannel,
  onFileStart: (item: vscode.TestItem | undefined) => void,
): void {
  switch (ev.event) {
    case 'file_start': {
      const item = itemMap.get(path.basename(ev.name));
      onFileStart(item);
      if (item) run.started(item);
      break;
    }
    case 'test_pass':
      outputChannel.appendLine(`  ✓ ${ev.name} (${formatUs(ev.us ?? 0)})`);
      break;
    case 'test_fail': {
      const msg = (ev.failures ?? []).join('\n');
      outputChannel.appendLine(`  ✗ ${ev.name}`);
      (ev.failures ?? []).forEach((f) => outputChannel.appendLine(`      → ${f}`));
      // Record failure against the file item (individual test items not discovered pre-run)
      const item = itemMap.get(ev.name.split('::')[0]?.split('/').pop() ?? '');
      if (item) {
        run.failed(item, new vscode.TestMessage(msg));
      }
      break;
    }
    case 'file_end': {
      const item = itemMap.get(path.basename(ev.name));
      if (item) run.passed(item);
      break;
    }
  }
}

function formatUs(us: number): string {
  if (us < 1000) return `${us} µs`;
  if (us < 1_000_000) return `${(us / 1000).toFixed(2)} ms`;
  return `${(us / 1_000_000).toFixed(2)} s`;
}

export function deactivate(): void {}
