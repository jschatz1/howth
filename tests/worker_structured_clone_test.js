// Test structured cloning in worker_threads (Buffer, TypedArray, Map, Set, Date, etc.)
const { Worker, isMainThread, workerData, parentPort } = require('worker_threads');

if (isMainThread) {
  console.log('Main thread: Testing structured cloning');

  // Test data with various types
  const testData = {
    buffer: Buffer.from([1, 2, 3, 4, 5]),
    uint8: new Uint8Array([10, 20, 30]),
    int32: new Int32Array([100, -200, 300]),
    float64: new Float64Array([1.5, 2.5, 3.5]),
    map: new Map([['key1', 'value1'], ['key2', 42]]),
    set: new Set([1, 2, 3, 'four']),
    date: new Date('2024-01-15T12:00:00Z'),
    regexp: /test.*pattern/gi,
    nested: {
      arr: [1, 'two', { three: 3 }],
      buf: Buffer.from('hello')
    }
  };

  const worker = new Worker(__filename, {
    workerData: testData
  });

  let testsPassed = 0;
  let testsFailed = 0;

  worker.on('message', (msg) => {
    if (msg.type === 'workerDataTest') {
      console.log('\n--- workerData Tests ---');
      for (const [name, passed] of Object.entries(msg.results)) {
        if (passed) {
          console.log(`✓ ${name}`);
          testsPassed++;
        } else {
          console.log(`✗ ${name}`);
          testsFailed++;
        }
      }
    } else if (msg.type === 'ready') {
      // Send message with complex types
      worker.postMessage({
        buffer: Buffer.from([100, 101, 102]),
        map: new Map([['sent', true]]),
        date: new Date('2025-06-01')
      });
    } else if (msg.type === 'messageTest') {
      console.log('\n--- postMessage Tests ---');
      for (const [name, passed] of Object.entries(msg.results)) {
        if (passed) {
          console.log(`✓ ${name}`);
          testsPassed++;
        } else {
          console.log(`✗ ${name}`);
          testsFailed++;
        }
      }

      console.log(`\n=== Results: ${testsPassed} passed, ${testsFailed} failed ===`);
      process.exit(testsFailed > 0 ? 1 : 0);
    }
  });

  worker.on('error', (err) => {
    console.log('✗ Worker error:', err);
    process.exit(1);
  });

} else {
  // Worker thread
  const results = {};

  // Test workerData types
  results['buffer is Buffer'] = Buffer.isBuffer(workerData.buffer);
  results['buffer content correct'] = workerData.buffer[0] === 1 && workerData.buffer[4] === 5;
  results['uint8 is Uint8Array'] = workerData.uint8 instanceof Uint8Array;
  results['uint8 content correct'] = workerData.uint8[0] === 10 && workerData.uint8[2] === 30;
  results['int32 is Int32Array'] = workerData.int32 instanceof Int32Array;
  results['int32 content correct'] = workerData.int32[1] === -200;
  results['float64 is Float64Array'] = workerData.float64 instanceof Float64Array;
  results['float64 content correct'] = workerData.float64[0] === 1.5;
  results['map is Map'] = workerData.map instanceof Map;
  results['map content correct'] = workerData.map.get('key1') === 'value1' && workerData.map.get('key2') === 42;
  results['set is Set'] = workerData.set instanceof Set;
  results['set content correct'] = workerData.set.has(1) && workerData.set.has('four');
  results['date is Date'] = workerData.date instanceof Date;
  results['date content correct'] = workerData.date.getFullYear() === 2024;
  results['regexp is RegExp'] = workerData.regexp instanceof RegExp;
  results['regexp flags correct'] = workerData.regexp.flags === 'gi';
  results['nested buffer is Buffer'] = Buffer.isBuffer(workerData.nested.buf);

  parentPort.postMessage({ type: 'workerDataTest', results });

  // Test receiving message
  parentPort.postMessage({ type: 'ready' });

  parentPort.on('message', (msg) => {
    const msgResults = {};
    msgResults['received buffer is Buffer'] = Buffer.isBuffer(msg.buffer);
    msgResults['received buffer content'] = msg.buffer[0] === 100;
    msgResults['received map is Map'] = msg.map instanceof Map;
    msgResults['received map content'] = msg.map.get('sent') === true;
    msgResults['received date is Date'] = msg.date instanceof Date;
    msgResults['received date year'] = msg.date.getFullYear() === 2025;

    parentPort.postMessage({ type: 'messageTest', results: msgResults });
  });
}
