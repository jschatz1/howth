/**
 * Worker Atomics Example
 *
 * Demonstrates SharedArrayBuffer and Atomics for thread-safe
 * communication between worker threads.
 */

const { Worker, isMainThread, parentPort, workerData } = require('worker_threads');
const path = require('path');

if (!isMainThread) {
  // Worker code
  const { sharedBuffer, workerId, iterations } = workerData;
  const counter = new Int32Array(sharedBuffer);

  // Each worker increments the shared counter
  for (let i = 0; i < iterations; i++) {
    Atomics.add(counter, 0, 1);
  }

  // Signal completion by incrementing the "done" counter
  Atomics.add(counter, 1, 1);

  // Notify any waiting threads
  Atomics.notify(counter, 1);

  parentPort.postMessage({
    workerId,
    finalValue: Atomics.load(counter, 0)
  });

  process.exit(0);
}

// Main thread
async function main() {
  console.log('=== Worker Atomics Example ===\n');

  const NUM_WORKERS = 4;
  const ITERATIONS_PER_WORKER = 10000;

  // Create shared memory:
  // - Int32 at index 0: shared counter
  // - Int32 at index 1: completion counter
  const sharedBuffer = new SharedArrayBuffer(8);
  const counter = new Int32Array(sharedBuffer);

  console.log(`Starting ${NUM_WORKERS} workers, each incrementing ${ITERATIONS_PER_WORKER} times`);
  console.log(`Expected final value: ${NUM_WORKERS * ITERATIONS_PER_WORKER}\n`);

  const startTime = Date.now();

  // Spawn workers
  const workers = [];
  const results = [];

  for (let i = 0; i < NUM_WORKERS; i++) {
    const worker = new Worker(__filename, {
      workerData: {
        sharedBuffer,
        workerId: i,
        iterations: ITERATIONS_PER_WORKER
      }
    });

    workers.push(new Promise((resolve, reject) => {
      worker.on('message', (msg) => {
        results.push(msg);
        resolve(msg);
      });
      worker.on('error', reject);
    }));
  }

  // Wait for all workers to complete
  await Promise.all(workers);

  const elapsed = Date.now() - startTime;
  const finalValue = Atomics.load(counter, 0);
  const completedWorkers = Atomics.load(counter, 1);

  console.log('Results:');
  results.sort((a, b) => a.workerId - b.workerId);
  for (const r of results) {
    console.log(`  Worker ${r.workerId}: saw counter at ${r.finalValue}`);
  }

  console.log(`\nFinal counter value: ${finalValue}`);
  console.log(`Workers completed: ${completedWorkers}`);
  console.log(`Time elapsed: ${elapsed}ms`);

  if (finalValue === NUM_WORKERS * ITERATIONS_PER_WORKER) {
    console.log('\n✓ SUCCESS: Counter matches expected value (no race conditions!)');
  } else {
    console.log('\n✗ ERROR: Counter mismatch - race condition detected');
    process.exit(1);
  }
}

main().catch(console.error);
