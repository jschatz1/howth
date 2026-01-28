'use strict';
const assert = require('assert');
const v8 = require('v8');

// Test getHeapStatistics
const heapStats = v8.getHeapStatistics();
assert.strictEqual(typeof heapStats, 'object');
assert.strictEqual(typeof heapStats.total_heap_size, 'number');
assert.strictEqual(typeof heapStats.used_heap_size, 'number');
assert.strictEqual(typeof heapStats.heap_size_limit, 'number');

// Test getHeapSpaceStatistics
const heapSpaces = v8.getHeapSpaceStatistics();
assert.ok(Array.isArray(heapSpaces));

// Test getHeapCodeStatistics
const codeStats = v8.getHeapCodeStatistics();
assert.strictEqual(typeof codeStats, 'object');
assert.strictEqual(typeof codeStats.code_and_metadata_size, 'number');

// Test setFlagsFromString (should not throw)
v8.setFlagsFromString('--max-old-space-size=512');

// Test serialize and deserialize
const testObj = { hello: 'world', num: 42, arr: [1, 2, 3] };
const serialized = v8.serialize(testObj);
assert.ok(Buffer.isBuffer(serialized));

const deserialized = v8.deserialize(serialized);
assert.deepStrictEqual(deserialized, testObj);

// Test cachedDataVersionTag
const tag = v8.cachedDataVersionTag();
assert.strictEqual(typeof tag, 'number');

// Test writeHeapSnapshot
const snapshotPath = v8.writeHeapSnapshot();
assert.strictEqual(typeof snapshotPath, 'string');

// Test takeCoverage and stopCoverage (should not throw)
v8.takeCoverage();
v8.stopCoverage();

console.log('All v8 tests passed!');
