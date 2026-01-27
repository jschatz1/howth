'use strict';
// Basic events tests
const assert = require('assert');
const EventEmitter = require('events');

// Test EventEmitter constructor
const emitter = new EventEmitter();
assert.strictEqual(typeof emitter, 'object');

// Test on/emit
let called = 0;
const handler = () => { called++; };
emitter.on('test', handler);
emitter.emit('test');
assert.strictEqual(called, 1);

// Test multiple emissions
emitter.emit('test');
assert.strictEqual(called, 2);

// Test off/removeListener
emitter.off('test', handler);
emitter.emit('test');
assert.strictEqual(called, 2); // Should not increment

// Test once
let onceCalled = 0;
emitter.once('once-test', () => { onceCalled++; });
emitter.emit('once-test');
emitter.emit('once-test');
assert.strictEqual(onceCalled, 1); // Should only be called once

// Test multiple handlers
let handler1Called = false;
let handler2Called = false;
emitter.on('multi', () => { handler1Called = true; });
emitter.on('multi', () => { handler2Called = true; });
emitter.emit('multi');
assert.strictEqual(handler1Called, true);
assert.strictEqual(handler2Called, true);

// Test emit return value
assert.strictEqual(emitter.emit('non-existent'), false);
assert.strictEqual(emitter.emit('multi'), true);

// Test removeAllListeners
emitter.removeAllListeners('multi');
let multiCallCount = 0;
emitter.on('multi', () => { multiCallCount++; });
emitter.emit('multi');
assert.strictEqual(multiCallCount, 1);

// Test addListener (alias for on)
let addListenerCalled = false;
emitter.addListener('add-test', () => { addListenerCalled = true; });
emitter.emit('add-test');
assert.strictEqual(addListenerCalled, true);

// Test listeners()
const listeners = emitter.listeners('multi');
assert.ok(Array.isArray(listeners));
assert.strictEqual(listeners.length, 1);

// Test listenerCount()
assert.strictEqual(emitter.listenerCount('multi'), 1);
assert.strictEqual(emitter.listenerCount('non-existent'), 0);

// Test event with arguments
let eventArgs = null;
emitter.on('args-test', (a, b, c) => {
  eventArgs = [a, b, c];
});
emitter.emit('args-test', 1, 'two', { three: 3 });
assert.deepStrictEqual(eventArgs, [1, 'two', { three: 3 }]);

// Test chaining
const chain = emitter
  .on('chain', () => {})
  .on('chain', () => {});
assert.strictEqual(chain, emitter);

console.log('All events tests passed!');
