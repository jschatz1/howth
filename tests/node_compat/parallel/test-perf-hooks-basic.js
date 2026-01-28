'use strict';
const assert = require('assert');
const { performance, PerformanceObserver } = require('perf_hooks');

// Test performance.now()
const now1 = performance.now();
assert.strictEqual(typeof now1, 'number');
assert.ok(now1 >= 0);

// Wait a bit and check that time advances
const start = performance.now();
let sum = 0;
for (let i = 0; i < 100000; i++) sum += i;
const end = performance.now();
assert.ok(end >= start);

// Test performance.timeOrigin
assert.strictEqual(typeof performance.timeOrigin, 'number');
assert.ok(performance.timeOrigin > 0);

// Test performance.mark()
const mark = performance.mark('test-mark');
assert.strictEqual(mark.name, 'test-mark');
assert.strictEqual(mark.entryType, 'mark');
assert.strictEqual(typeof mark.startTime, 'number');

// Test performance.measure()
performance.mark('start-mark');
performance.mark('end-mark');
const measure = performance.measure('test-measure', 'start-mark', 'end-mark');
assert.strictEqual(measure.name, 'test-measure');
assert.strictEqual(measure.entryType, 'measure');
assert.strictEqual(typeof measure.duration, 'number');

// Test getEntries
const entries = performance.getEntries();
assert.ok(Array.isArray(entries));
assert.ok(entries.length > 0);

// Test getEntriesByName
const markEntries = performance.getEntriesByName('test-mark');
assert.ok(markEntries.length > 0);
assert.strictEqual(markEntries[0].name, 'test-mark');

// Test getEntriesByType
const marks = performance.getEntriesByType('mark');
assert.ok(marks.length > 0);

// Test clearMarks
performance.clearMarks('test-mark');
const clearedMarks = performance.getEntriesByName('test-mark');
assert.strictEqual(clearedMarks.length, 0);

// Test clearMeasures
performance.clearMeasures();

// Test PerformanceObserver exists
assert.strictEqual(typeof PerformanceObserver, 'function');

// Test toJSON
const json = performance.toJSON();
assert.strictEqual(typeof json, 'object');
assert.strictEqual(typeof json.timeOrigin, 'number');

console.log('All perf_hooks tests passed!');
