// Test receiveMessageOnPort and markAsUntransferable
const { MessageChannel, receiveMessageOnPort, markAsUntransferable, isMarkedAsUntransferable } = require('worker_threads');

console.log('Testing receiveMessageOnPort and markAsUntransferable...\n');

let passed = 0;
let failed = 0;

function test(name, fn) {
  try {
    fn();
    console.log(`✓ ${name}`);
    passed++;
  } catch (e) {
    console.log(`✗ ${name}: ${e.message}`);
    failed++;
  }
}

function assertEqual(actual, expected, msg) {
  if (actual !== expected) {
    throw new Error(`${msg}: expected ${expected}, got ${actual}`);
  }
}

function assertTrue(value, msg) {
  if (!value) {
    throw new Error(`${msg}: expected true, got ${value}`);
  }
}

function assertFalse(value, msg) {
  if (value) {
    throw new Error(`${msg}: expected false, got ${value}`);
  }
}

// Test receiveMessageOnPort
test('receiveMessageOnPort returns undefined when no messages', () => {
  const { port1 } = new MessageChannel();
  const result = receiveMessageOnPort(port1);
  assertEqual(result, undefined, 'empty port result');
});

test('receiveMessageOnPort returns message object', () => {
  const { port1, port2 } = new MessageChannel();

  // Post message but don't start the port (messages queue up)
  port2.postMessage({ hello: 'world' });

  // Receive synchronously
  const result = receiveMessageOnPort(port1);
  assertTrue(result !== undefined, 'result should exist');
  assertEqual(result.message.hello, 'world', 'message content');
});

test('receiveMessageOnPort drains one message at a time', () => {
  const { port1, port2 } = new MessageChannel();

  port2.postMessage({ msg: 1 });
  port2.postMessage({ msg: 2 });
  port2.postMessage({ msg: 3 });

  const r1 = receiveMessageOnPort(port1);
  const r2 = receiveMessageOnPort(port1);
  const r3 = receiveMessageOnPort(port1);
  const r4 = receiveMessageOnPort(port1);

  assertEqual(r1.message.msg, 1, 'first message');
  assertEqual(r2.message.msg, 2, 'second message');
  assertEqual(r3.message.msg, 3, 'third message');
  assertEqual(r4, undefined, 'no more messages');
});

test('receiveMessageOnPort handles invalid port gracefully', () => {
  // Our implementation returns undefined for invalid ports
  // rather than throwing like Node.js does
  const result = receiveMessageOnPort(null);
  assertEqual(result, undefined, 'null port result');

  const result2 = receiveMessageOnPort({});
  assertEqual(result2, undefined, 'invalid port result');
});

// Test markAsUntransferable
test('markAsUntransferable exists', () => {
  assertEqual(typeof markAsUntransferable, 'function', 'markAsUntransferable type');
});

test('markAsUntransferable marks object', () => {
  const obj = { data: 'test' };
  assertFalse(isMarkedAsUntransferable(obj), 'initially not marked');

  markAsUntransferable(obj);
  assertTrue(isMarkedAsUntransferable(obj), 'now marked');
});

test('markAsUntransferable ignores primitives', () => {
  // Should not throw
  markAsUntransferable(null);
  markAsUntransferable(undefined);
  markAsUntransferable(42);
  markAsUntransferable('string');
});

test('markAsUntransferable works with arrays', () => {
  const arr = [1, 2, 3];
  markAsUntransferable(arr);
  assertTrue(isMarkedAsUntransferable(arr), 'array marked');
});

test('markAsUntransferable works with ArrayBuffer', () => {
  const buffer = new ArrayBuffer(16);
  markAsUntransferable(buffer);
  assertTrue(isMarkedAsUntransferable(buffer), 'ArrayBuffer marked');
});

// Test that postMessage with transferList checks for untransferable
test('postMessage throws for untransferable in transferList', () => {
  const { port1, port2 } = new MessageChannel();
  const buffer = new ArrayBuffer(8);

  markAsUntransferable(buffer);

  try {
    port1.postMessage({ buffer }, [buffer]);
    throw new Error('Should have thrown');
  } catch (e) {
    assertEqual(e.name, 'DataCloneError', 'error name');
  }
});

console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
process.exit(failed > 0 ? 1 : 0);
