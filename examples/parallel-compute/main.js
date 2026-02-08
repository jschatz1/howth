/**
 * Parallel Compute Example
 *
 * Demonstrates splitting CPU-intensive work across multiple
 * worker threads with shared result aggregation.
 */

const { Worker, isMainThread, parentPort, workerData } = require('worker_threads');

if (!isMainThread) {
  // Worker: compute sum of numbers in range
  const { start, end, resultBuffer, resultIndex } = workerData;
  const results = new Float64Array(resultBuffer);

  let sum = 0;
  for (let i = start; i < end; i++) {
    // Simulate some computation (e.g., checking primality-ish)
    sum += Math.sqrt(i) * Math.sin(i);
  }

  // Store result in shared buffer
  results[resultIndex] = sum;

  parentPort.postMessage({ start, end, sum });
  process.exit(0);
}

// Main thread
async function main() {
  console.log('=== Parallel Compute Example ===\n');

  const TOTAL_NUMBERS = 10_000_000;
  const NUM_WORKERS = 4;
  const CHUNK_SIZE = Math.ceil(TOTAL_NUMBERS / NUM_WORKERS);

  // Shared buffer for results (one Float64 per worker)
  const resultBuffer = new SharedArrayBuffer(NUM_WORKERS * 8);
  const results = new Float64Array(resultBuffer);

  console.log(`Computing sum of f(x) = sqrt(x) * sin(x) for x in [0, ${TOTAL_NUMBERS})`);
  console.log(`Splitting across ${NUM_WORKERS} workers (${CHUNK_SIZE} numbers each)\n`);

  // Sequential baseline
  console.log('Running sequential baseline...');
  const seqStart = Date.now();
  let sequentialSum = 0;
  for (let i = 0; i < TOTAL_NUMBERS; i++) {
    sequentialSum += Math.sqrt(i) * Math.sin(i);
  }
  const seqTime = Date.now() - seqStart;
  console.log(`  Sequential: ${seqTime}ms\n`);

  // Parallel computation
  console.log('Running parallel computation...');
  const parStart = Date.now();

  const workers = [];
  for (let i = 0; i < NUM_WORKERS; i++) {
    const start = i * CHUNK_SIZE;
    const end = Math.min(start + CHUNK_SIZE, TOTAL_NUMBERS);

    const worker = new Worker(__filename, {
      workerData: {
        start,
        end,
        resultBuffer,
        resultIndex: i
      }
    });

    workers.push(new Promise((resolve, reject) => {
      worker.on('message', resolve);
      worker.on('error', reject);
    }));
  }

  const workerResults = await Promise.all(workers);
  const parTime = Date.now() - parStart;

  // Sum up results from shared buffer
  let parallelSum = 0;
  for (let i = 0; i < NUM_WORKERS; i++) {
    parallelSum += results[i];
  }

  console.log('  Worker results:');
  for (const r of workerResults) {
    console.log(`    [${r.start.toLocaleString()} - ${r.end.toLocaleString()}): ${r.sum.toFixed(4)}`);
  }

  console.log(`\n  Parallel: ${parTime}ms`);
  console.log(`  Speedup: ${(seqTime / parTime).toFixed(2)}x\n`);

  console.log('Results:');
  console.log(`  Sequential sum: ${sequentialSum.toFixed(6)}`);
  console.log(`  Parallel sum:   ${parallelSum.toFixed(6)}`);

  const diff = Math.abs(sequentialSum - parallelSum);
  if (diff < 0.0001) {
    console.log(`\n✓ Results match (diff: ${diff.toExponential(2)})`);
  } else {
    console.log(`\n✗ Results differ by ${diff}`);
  }
}

main().catch(console.error);
