// Test workerData passing
const { Worker, isMainThread, workerData } = require('worker_threads');

if (isMainThread) {
  console.log('Main thread: Creating worker with workerData');

  const worker = new Worker(__filename, {
    workerData: {
      name: 'TestWorker',
      count: 42,
      nested: { foo: 'bar' }
    }
  });

  worker.on('message', (msg) => {
    console.log('Main received:', JSON.stringify(msg));
    if (msg.success) {
      console.log('✓ workerData test passed!');
      process.exit(0);
    } else {
      console.log('✗ workerData test failed:', msg.error);
      process.exit(1);
    }
  });

  worker.on('error', (err) => {
    console.log('✗ Worker error:', err);
    process.exit(1);
  });

} else {
  // Worker thread
  console.log('Worker thread: workerData =', JSON.stringify(workerData));

  const { parentPort } = require('worker_threads');

  // Verify workerData was passed correctly
  if (workerData &&
      workerData.name === 'TestWorker' &&
      workerData.count === 42 &&
      workerData.nested &&
      workerData.nested.foo === 'bar') {
    parentPort.postMessage({ success: true, received: workerData });
  } else {
    parentPort.postMessage({
      success: false,
      error: 'workerData not received correctly',
      received: workerData
    });
  }
}
