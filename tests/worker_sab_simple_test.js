// Simple test for SharedArrayBuffer in workerData
const { Worker, isMainThread, parentPort, workerData } = require('worker_threads');

if (!isMainThread) {
  console.log('Worker: received workerData:', typeof workerData);
  console.log('Worker: workerData.sab:', typeof workerData.sab);

  if (workerData.sab && workerData.sab instanceof SharedArrayBuffer) {
    console.log('Worker: SAB byteLength:', workerData.sab.byteLength);
    console.log('Worker: SAB has __howthSharedBufferId:', !!workerData.sab.__howthSharedBufferId);

    const view = new Int32Array(workerData.sab);
    console.log('Worker: Initial value at index 0:', view[0]);

    // Just use regular Atomics first
    Atomics.add(view, 0, 10);
    console.log('Worker: After adding 10:', Atomics.load(view, 0));

    parentPort.postMessage({ success: true, value: Atomics.load(view, 0) });
  } else {
    parentPort.postMessage({ success: false, error: 'SAB not received properly' });
  }

  process.exit(0);
}

// Main thread
console.log('Main: Creating SharedArrayBuffer...');
const sab = new SharedArrayBuffer(16);
const view = new Int32Array(sab);
view[0] = 100;
console.log('Main: Initial value:', view[0]);
console.log('Main: SAB byteLength:', sab.byteLength);

console.log('Main: Creating worker...');
const worker = new Worker(__filename, {
  workerData: { sab }
});

worker.on('message', (msg) => {
  console.log('Main: Received from worker:', msg);
  if (msg.success) {
    console.log('Main: Value after worker:', view[0]);
    console.log('✓ Test passed!');
  } else {
    console.log('✗ Test failed:', msg.error);
  }
  process.exit(msg.success ? 0 : 1);
});

worker.on('error', (err) => {
  console.log('Main: Worker error:', err);
  process.exit(1);
});

worker.on('exit', (code) => {
  console.log('Main: Worker exited with code:', code);
});
