'use strict';
const assert = require('assert');
const timers = require('timers');

// Test setTimeout exists
assert.strictEqual(typeof timers.setTimeout, 'function');
assert.strictEqual(typeof timers.clearTimeout, 'function');

// Test setInterval exists
assert.strictEqual(typeof timers.setInterval, 'function');
assert.strictEqual(typeof timers.clearInterval, 'function');

// Test setImmediate exists
assert.strictEqual(typeof timers.setImmediate, 'function');
assert.strictEqual(typeof timers.clearImmediate, 'function');

// Test promises API
const timersPromises = require('timers/promises');
assert.strictEqual(typeof timersPromises.setTimeout, 'function');
assert.strictEqual(typeof timersPromises.setImmediate, 'function');

// Test setTimeout
let timeoutCalled = false;
timers.setTimeout(() => {
  timeoutCalled = true;
}, 10);

// Test clearTimeout
let clearedTimeoutCalled = false;
const timeoutId = timers.setTimeout(() => {
  clearedTimeoutCalled = true;
}, 10);
timers.clearTimeout(timeoutId);

// Test setImmediate
let immediateCalled = false;
timers.setImmediate(() => {
  immediateCalled = true;
});

// Test timers/promises setTimeout
timersPromises.setTimeout(10, 'test-value').then((value) => {
  assert.strictEqual(value, 'test-value');

  // Verify all timers ran correctly
  setTimeout(() => {
    assert.strictEqual(timeoutCalled, true, 'setTimeout callback should have been called');
    assert.strictEqual(clearedTimeoutCalled, false, 'cleared timeout should not have been called');
    assert.strictEqual(immediateCalled, true, 'setImmediate callback should have been called');
    console.log('All timers module tests passed!');
  }, 50);
});
