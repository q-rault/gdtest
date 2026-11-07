import assert from 'node:assert/strict';
import { test } from 'node:test';
import { detailsOf, formatUs, outcomeOf, parseEvent, projectRelativePath, testTarget } from './results';

test('parses the events the CLI writes', () => {
  const event = parseEvent(
    '{"kind":"test_fail","worker":0,"file":"res://test/a.gd","name":"test_bad","us":16,"failures":["expected 2 == 1"]}',
  );
  assert.equal(event?.kind, 'test_fail');
  assert.equal(event?.name, 'test_bad');
  assert.deepEqual(detailsOf(event!), ['expected 2 == 1']);
});

test('ignores anything that is not an event', () => {
  assert.equal(parseEvent(''), undefined);
  assert.equal(parseEvent('gdtest: building Godot class cache'), undefined);
  assert.equal(parseEvent('{not json'), undefined);
  assert.equal(parseEvent('{"no_kind":true}'), undefined);
});

test('maps events to outcomes', () => {
  assert.equal(outcomeOf({ kind: 'test_pass' }), 'passed');
  assert.equal(outcomeOf({ kind: 'test_fail' }), 'failed');
  assert.equal(outcomeOf({ kind: 'test_error' }), 'errored');
  assert.equal(outcomeOf({ kind: 'test_skip' }), 'skipped');
  assert.equal(outcomeOf({ kind: 'test_pending' }), 'skipped');
  assert.equal(outcomeOf({ kind: 'test_start' }), undefined);
  assert.equal(outcomeOf({ kind: 'summary' }), undefined);
});

test('a script error explains itself through its message', () => {
  assert.deepEqual(detailsOf({ kind: 'test_error', message: 'script error: boom' }), ['script error: boom']);
  assert.deepEqual(detailsOf({ kind: 'test_pass' }), []);
});

test('builds targets with forward slashes on every platform', () => {
  assert.equal(projectRelativePath('res://test/a.gd'), 'test/a.gd');
  assert.equal(testTarget('test\\sub\\a.gd'), 'test/sub/a.gd');
  assert.equal(testTarget('test/a.gd', 'test_x'), 'test/a.gd::test_x');
});

test('formats durations', () => {
  assert.equal(formatUs(7), '7 µs');
  assert.equal(formatUs(1500), '1.50 ms');
  assert.equal(formatUs(2_000_000), '2.00 s');
});
