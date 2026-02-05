// Test BroadcastChannel implementation
const { BroadcastChannel } = require('worker_threads');

console.log('Testing BroadcastChannel...\n');

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

// Test 1: BroadcastChannel exists
test('BroadcastChannel is available from worker_threads', () => {
  assertEqual(typeof BroadcastChannel, 'function', 'BroadcastChannel type');
});

// Test 2: BroadcastChannel is also a global
test('BroadcastChannel is available as global', () => {
  assertEqual(typeof globalThis.BroadcastChannel, 'function', 'global BroadcastChannel type');
});

// Test 3: Create a channel
test('Can create a BroadcastChannel', () => {
  const channel = new BroadcastChannel('test-channel');
  assertEqual(channel.name, 'test-channel', 'channel name');
  channel.close();
});

// Test 4: Message delivery between channels with same name
test('Messages are delivered between channels with same name', (done) => {
  return new Promise((resolve, reject) => {
    const sender = new BroadcastChannel('msg-test');
    const receiver = new BroadcastChannel('msg-test');

    receiver.addEventListener('message', (event) => {
      try {
        assertEqual(event.data.hello, 'world', 'message data');
        sender.close();
        receiver.close();
        resolve();
      } catch (e) {
        reject(e);
      }
    });

    sender.postMessage({ hello: 'world' });

    // Timeout
    setTimeout(() => reject(new Error('Timeout waiting for message')), 1000);
  });
});

// Test 5: Messages not delivered to channels with different names
test('Messages not delivered to channels with different names', () => {
  return new Promise((resolve, reject) => {
    const sender = new BroadcastChannel('channel-a');
    const receiver = new BroadcastChannel('channel-b');
    let received = false;

    receiver.addEventListener('message', () => {
      received = true;
    });

    sender.postMessage({ test: true });

    // Give time for potential delivery
    setTimeout(() => {
      sender.close();
      receiver.close();
      if (received) {
        reject(new Error('Message was incorrectly delivered'));
      } else {
        resolve();
      }
    }, 100);
  });
});

// Test 6: Sender doesn't receive own messages
test('Sender does not receive own messages', () => {
  return new Promise((resolve, reject) => {
    const channel = new BroadcastChannel('self-test');
    let received = false;

    channel.addEventListener('message', () => {
      received = true;
    });

    channel.postMessage({ test: true });

    setTimeout(() => {
      channel.close();
      if (received) {
        reject(new Error('Sender received own message'));
      } else {
        resolve();
      }
    }, 100);
  });
});

// Test 7: Closed channel throws on postMessage
test('Closed channel throws on postMessage', () => {
  const channel = new BroadcastChannel('closed-test');
  channel.close();

  try {
    channel.postMessage({ test: true });
    throw new Error('Should have thrown');
  } catch (e) {
    if (e.name !== 'InvalidStateError') {
      throw new Error(`Expected InvalidStateError, got ${e.name}`);
    }
  }
});

// Test 8: onmessage handler works
test('onmessage handler receives messages', () => {
  return new Promise((resolve, reject) => {
    const sender = new BroadcastChannel('onmessage-test');
    const receiver = new BroadcastChannel('onmessage-test');

    receiver.onmessage = (event) => {
      try {
        assertEqual(event.data.value, 42, 'onmessage data');
        sender.close();
        receiver.close();
        resolve();
      } catch (e) {
        reject(e);
      }
    };

    sender.postMessage({ value: 42 });

    setTimeout(() => reject(new Error('Timeout')), 1000);
  });
});

// Run async tests
async function runTests() {
  // Sync tests already ran above, now run async ones
  const asyncTests = [
    ['Messages are delivered between channels with same name', async () => {
      const sender = new BroadcastChannel('msg-test');
      const receiver = new BroadcastChannel('msg-test');

      return new Promise((resolve, reject) => {
        receiver.addEventListener('message', (event) => {
          try {
            assertEqual(event.data.hello, 'world', 'message data');
            sender.close();
            receiver.close();
            resolve();
          } catch (e) {
            reject(e);
          }
        });

        sender.postMessage({ hello: 'world' });
        setTimeout(() => reject(new Error('Timeout')), 1000);
      });
    }],
    ['Messages not delivered to channels with different names', async () => {
      const sender = new BroadcastChannel('channel-a');
      const receiver = new BroadcastChannel('channel-b');
      let received = false;

      receiver.addEventListener('message', () => { received = true; });
      sender.postMessage({ test: true });

      await new Promise(r => setTimeout(r, 100));
      sender.close();
      receiver.close();
      if (received) throw new Error('Message was incorrectly delivered');
    }],
    ['Sender does not receive own messages', async () => {
      const channel = new BroadcastChannel('self-test');
      let received = false;

      channel.addEventListener('message', () => { received = true; });
      channel.postMessage({ test: true });

      await new Promise(r => setTimeout(r, 100));
      channel.close();
      if (received) throw new Error('Sender received own message');
    }],
    ['onmessage handler receives messages', async () => {
      const sender = new BroadcastChannel('onmessage-test');
      const receiver = new BroadcastChannel('onmessage-test');

      return new Promise((resolve, reject) => {
        receiver.onmessage = (event) => {
          try {
            assertEqual(event.data.value, 42, 'onmessage data');
            sender.close();
            receiver.close();
            resolve();
          } catch (e) {
            reject(e);
          }
        };

        sender.postMessage({ value: 42 });
        setTimeout(() => reject(new Error('Timeout')), 1000);
      });
    }],
  ];

  for (const [name, fn] of asyncTests) {
    try {
      await fn();
      console.log(`✓ ${name}`);
      passed++;
    } catch (e) {
      console.log(`✗ ${name}: ${e.message}`);
      failed++;
    }
  }

  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
  process.exit(failed > 0 ? 1 : 0);
}

runTests();
