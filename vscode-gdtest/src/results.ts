// Reading `gdtest --reporter json`, which writes one JSON object per line. Kept free
// of the VS Code API so that it can be tested on its own.

export interface GdtestEvent {
  kind: string;
  file?: string;
  name?: string;
  us?: number;
  failures?: string[];
  message?: string;
  pass?: number;
  fail?: number;
  error?: number;
  pending?: number;
  skipped?: number;
}

export type Outcome = 'passed' | 'failed' | 'errored' | 'skipped';

export function parseEvent(line: string): GdtestEvent | undefined {
  const text = line.trim();
  if (!text.startsWith('{')) return undefined;
  try {
    const event = JSON.parse(text);
    return typeof event.kind === 'string' ? (event as GdtestEvent) : undefined;
  } catch {
    return undefined;
  }
}

/** The outcome an event records for its test, if it is a test result. */
export function outcomeOf(event: GdtestEvent): Outcome | undefined {
  switch (event.kind) {
    case 'test_pass':
      return 'passed';
    case 'test_fail':
      return 'failed';
    case 'test_error':
      return 'errored';
    case 'test_skip':
    case 'test_pending':
      return 'skipped';
    default:
      return undefined;
  }
}

/** Why a test did not pass, as lines to show to the user. */
export function detailsOf(event: GdtestEvent): string[] {
  if (event.failures && event.failures.length > 0) return event.failures;
  return event.message ? [event.message] : [];
}

/** `res://test/a.gd` to `test/a.gd`. */
export function projectRelativePath(file: string): string {
  return file.replace(/^res:\/\//, '');
}

/** The command line target for a file, or for one test in it. */
export function testTarget(relativePath: string, testName?: string): string {
  const file = relativePath.replace(/\\/g, '/');
  return testName ? `${file}::${testName}` : file;
}

export function formatUs(us: number): string {
  if (us < 1000) return `${us} µs`;
  if (us < 1_000_000) return `${(us / 1000).toFixed(2)} ms`;
  return `${(us / 1_000_000).toFixed(2)} s`;
}
