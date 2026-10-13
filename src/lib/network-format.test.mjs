import assert from 'node:assert/strict';
import { test } from 'node:test';
import { formatBytes, formatSpeed } from './network-format.ts';

test('low speeds have at most one decimal and no trailing .0', () => {
  for (const [value, expected] of [
    [0, '0'], [0.04, '0B/s'], [0.1, '0.1B/s'], [1, '1B/s'],
    [1.25, '1.3B/s'], [414.1446480599786, '414.1B/s'],
    [496.41436088560323, '496.4B/s'], [414, '414B/s'],
    [1023, '1023B/s'], [1023.99, '1024B/s'],
  ]) assert.equal(formatSpeed(value), expected);
});

test('larger speeds and cumulative-byte formatting keep their existing output', () => {
  assert.equal(formatSpeed(1024), '1.0K/s');
  assert.equal(formatSpeed(1024 ** 2), '1.0M/s');
  assert.equal(formatSpeed(1024 ** 3), '1.00G/s');
  assert.equal(formatBytes(0), '0');
  assert.equal(formatBytes(414), '414B');
  assert.equal(formatBytes('1023'), '1023B');
  assert.equal(formatBytes('1024'), '1.0K');
  assert.equal(formatBytes('1073741824'), '1.00G');
});
