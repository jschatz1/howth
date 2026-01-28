'use strict';
const assert = require('assert');
const async_hooks = require('async_hooks');

// Test createHook
const hook = async_hooks.createHook({
  init: () => {},
  before: () => {},
  after: () => {},
  destroy: () => {},
});
assert.strictEqual(typeof hook.enable, 'function');
assert.strictEqual(typeof hook.disable, 'function');

// Test executionAsyncId
const asyncId = async_hooks.executionAsyncId();
assert.strictEqual(typeof asyncId, 'number');

// Test triggerAsyncId
const triggerId = async_hooks.triggerAsyncId();
assert.strictEqual(typeof triggerId, 'number');

// Test executionAsyncResource
const resource = async_hooks.executionAsyncResource();
assert.strictEqual(typeof resource, 'object');

// Test AsyncResource
const { AsyncResource } = async_hooks;
const asyncResource = new AsyncResource('TEST');
assert.strictEqual(asyncResource.type, 'TEST');
assert.strictEqual(typeof asyncResource.asyncId, 'number');

// Test runInAsyncScope
let scopeRan = false;
asyncResource.runInAsyncScope(() => {
  scopeRan = true;
});
assert.strictEqual(scopeRan, true);

// Test runInAsyncScope with return value
const result = asyncResource.runInAsyncScope(() => 42);
assert.strictEqual(result, 42);

// Test emitDestroy
assert.strictEqual(asyncResource.emitDestroy(), asyncResource);

// Test bind
const boundFn = asyncResource.bind(() => 'bound');
assert.strictEqual(boundFn(), 'bound');

// Test AsyncLocalStorage
const { AsyncLocalStorage } = async_hooks;
const asyncLocalStorage = new AsyncLocalStorage();

// Test run
let storeValue = null;
asyncLocalStorage.run({ value: 'test' }, () => {
  storeValue = asyncLocalStorage.getStore();
});
assert.deepStrictEqual(storeValue, { value: 'test' });

// Test getStore outside of run
assert.strictEqual(asyncLocalStorage.getStore(), undefined);

// Test nested run
asyncLocalStorage.run({ outer: true }, () => {
  assert.deepStrictEqual(asyncLocalStorage.getStore(), { outer: true });
  asyncLocalStorage.run({ inner: true }, () => {
    assert.deepStrictEqual(asyncLocalStorage.getStore(), { inner: true });
  });
  assert.deepStrictEqual(asyncLocalStorage.getStore(), { outer: true });
});

// Test exit
asyncLocalStorage.run({ value: 'test' }, () => {
  asyncLocalStorage.exit(() => {
    assert.strictEqual(asyncLocalStorage.getStore(), undefined);
  });
  assert.deepStrictEqual(asyncLocalStorage.getStore(), { value: 'test' });
});

// Test enterWith
asyncLocalStorage.enterWith({ entered: true });
assert.deepStrictEqual(asyncLocalStorage.getStore(), { entered: true });

// Test disable
asyncLocalStorage.disable();
assert.strictEqual(asyncLocalStorage.getStore(), undefined);

console.log('All async_hooks tests passed!');
