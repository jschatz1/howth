// Test SharedArrayBuffer transfer and SharedAtomics between workers
const { Worker, isMainThread, parentPort, workerData, SharedAtomics } = require('worker_threads');
const path = require('path');

if (!isMainThread) {
  // Worker code
  const { sab, testIndex } = workerData;
  const view = new Int32Array(sab);

  // Use SharedAtomics for cross-worker atomic operations
  const oldValue = SharedAtomics.add(view, testIndex, 10);

  parentPort.postMessage({
    oldValue,
    newValue: SharedAtomics.load(view, testIndex),
    threadType: 'worker'
  });

  process.exit(0);
}

// Main thread code
console.log('Testing SharedArrayBuffer transfer and SharedAtomics...\n');

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

// Test 1: Basic SharedAtomics in main thread
test('SharedAtomics.store and load', () => {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);

  SharedAtomics.store(view, 0, 42);
  const value = SharedAtomics.load(view, 0);

  if (value !== 42) throw new Error(`Expected 42, got ${value}`);
});

test('SharedAtomics.add', () => {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);

  SharedAtomics.store(view, 0, 10);
  const oldValue = SharedAtomics.add(view, 0, 5);
  const newValue = SharedAtomics.load(view, 0);

  if (oldValue !== 10) throw new Error(`Expected old value 10, got ${oldValue}`);
  if (newValue !== 15) throw new Error(`Expected new value 15, got ${newValue}`);
});

test('SharedAtomics.compareExchange', () => {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);

  SharedAtomics.store(view, 0, 50);

  // Should succeed
  const result1 = SharedAtomics.compareExchange(view, 0, 50, 75);
  if (result1 !== 50) throw new Error(`CAS should return old value 50, got ${result1}`);
  if (SharedAtomics.load(view, 0) !== 75) throw new Error('CAS should have set value to 75');

  // Should fail
  const result2 = SharedAtomics.compareExchange(view, 0, 50, 100);
  if (result2 !== 75) throw new Error(`CAS should return current value 75, got ${result2}`);
  if (SharedAtomics.load(view, 0) !== 75) throw new Error('CAS should not have changed value');
});

// Test: SharedArrayBuffer transfer to worker
async function testWorkerTransfer() {
  return new Promise((resolve, reject) => {
    const sab = new SharedArrayBuffer(16);
    const view = new Int32Array(sab);

    // Initialize with a value
    SharedAtomics.store(view, 0, 100);

    const worker = new Worker(__filename, {
      workerData: { sab, testIndex: 0 }
    });

    worker.on('message', (msg) => {
      try {
        // Worker should have added 10
        if (msg.oldValue !== 100) {
          reject(new Error(`Worker saw old value ${msg.oldValue}, expected 100`));
          return;
        }
        if (msg.newValue !== 110) {
          reject(new Error(`Worker saw new value ${msg.newValue}, expected 110`));
          return;
        }

        // Main thread should see the updated value
        const mainValue = SharedAtomics.load(view, 0);
        if (mainValue !== 110) {
          reject(new Error(`Main thread sees ${mainValue}, expected 110`));
          return;
        }

        resolve();
      } catch (e) {
        reject(e);
      }
    });

    worker.on('error', reject);
  });
}

// Run async test
testWorkerTransfer()
  .then(() => {
    console.log('✓ SharedArrayBuffer transfer to worker with atomic operations');
    passed++;
  })
  .catch((e) => {
    console.log(`✗ SharedArrayBuffer transfer to worker: ${e.message}`);
    failed++;
  })
  .finally(() => {
    console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
    process.exit(failed > 0 ? 1 : 0);
  });
