/**
 * Data Pipeline Example
 *
 * Demonstrates stream processing with a worker pool.
 * Data flows through multiple processing stages handled by workers.
 */

const { Worker, isMainThread, parentPort, workerData } = require('worker_threads');

if (!isMainThread) {
  const { stage } = workerData;

  // Each stage performs a different transformation
  const processors = {
    parse: (data) => {
      // Parse JSON-like strings
      return data.map(item => ({
        id: item.id,
        value: parseFloat(item.raw),
        timestamp: Date.now()
      }));
    },

    transform: (data) => {
      // Apply transformations
      return data.map(item => ({
        ...item,
        normalized: item.value / 100,
        squared: item.value * item.value
      }));
    },

    filter: (data) => {
      // Filter based on criteria
      return data.filter(item => item.value > 25 && item.value < 75);
    },

    aggregate: (data) => {
      // Compute aggregates
      const sum = data.reduce((acc, item) => acc + item.value, 0);
      const avg = data.length > 0 ? sum / data.length : 0;
      return {
        count: data.length,
        sum,
        avg,
        min: Math.min(...data.map(d => d.value)),
        max: Math.max(...data.map(d => d.value))
      };
    }
  };

  parentPort.on('message', (batch) => {
    const processor = processors[stage];
    if (processor) {
      const result = processor(batch);
      parentPort.postMessage(result);
    }
  });
}

// Main thread: orchestrate the pipeline
async function main() {
  console.log('=== Data Pipeline Example ===\n');
  console.log('Processing data through multiple worker stages:\n');
  console.log('  Raw Data → [Parse] → [Transform] → [Filter] → [Aggregate]\n');

  const stages = ['parse', 'transform', 'filter', 'aggregate'];

  // Create worker for each stage
  const workers = {};
  for (const stage of stages) {
    workers[stage] = new Worker(__filename, {
      workerData: { stage }
    });
  }

  // Helper to send data through a stage
  function processStage(stage, data) {
    return new Promise((resolve, reject) => {
      const worker = workers[stage];
      worker.once('message', resolve);
      worker.once('error', reject);
      worker.postMessage(data);
    });
  }

  // Generate sample data
  const BATCH_COUNT = 5;
  const ITEMS_PER_BATCH = 100;

  console.log(`Generating ${BATCH_COUNT} batches of ${ITEMS_PER_BATCH} items each...\n`);

  const batches = [];
  for (let b = 0; b < BATCH_COUNT; b++) {
    const batch = [];
    for (let i = 0; i < ITEMS_PER_BATCH; i++) {
      batch.push({
        id: `${b}-${i}`,
        raw: (Math.random() * 100).toString()
      });
    }
    batches.push(batch);
  }

  // Process all batches through the pipeline
  console.log('Processing batches through pipeline...\n');

  const startTime = Date.now();
  const results = [];

  for (let i = 0; i < batches.length; i++) {
    let data = batches[i];

    console.log(`Batch ${i + 1}/${BATCH_COUNT}:`);

    // Stage 1: Parse
    data = await processStage('parse', data);
    console.log(`  [Parse] → ${data.length} items`);

    // Stage 2: Transform
    data = await processStage('transform', data);
    console.log(`  [Transform] → ${data.length} items`);

    // Stage 3: Filter
    data = await processStage('filter', data);
    console.log(`  [Filter] → ${data.length} items (filtered from ${ITEMS_PER_BATCH})`);

    // Stage 4: Aggregate
    const aggregated = await processStage('aggregate', data);
    console.log(`  [Aggregate] → count=${aggregated.count}, avg=${aggregated.avg.toFixed(2)}`);

    results.push(aggregated);
  }

  const elapsed = Date.now() - startTime;

  // Final aggregation across all batches
  console.log('\n=== Final Results ===\n');

  const totalCount = results.reduce((acc, r) => acc + r.count, 0);
  const totalSum = results.reduce((acc, r) => acc + r.sum, 0);
  const globalAvg = totalCount > 0 ? totalSum / totalCount : 0;
  const globalMin = Math.min(...results.map(r => r.min));
  const globalMax = Math.max(...results.map(r => r.max));

  console.log(`Total items processed: ${BATCH_COUNT * ITEMS_PER_BATCH}`);
  console.log(`Items after filtering: ${totalCount}`);
  console.log(`Global average: ${globalAvg.toFixed(2)}`);
  console.log(`Global range: [${globalMin.toFixed(2)}, ${globalMax.toFixed(2)}]`);
  console.log(`\nTotal time: ${elapsed}ms`);
  console.log(`Throughput: ${((BATCH_COUNT * ITEMS_PER_BATCH) / (elapsed / 1000)).toFixed(0)} items/sec`);

  console.log('\n✓ Pipeline processing complete!');

  // Terminate workers
  for (const worker of Object.values(workers)) {
    worker.terminate();
  }
}

main().catch(console.error);
