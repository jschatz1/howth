// Test worker_threads resourceLimits
const { Worker, isMainThread, resourceLimits } = require('worker_threads');

if (isMainThread) {
  console.log('Testing worker_threads resourceLimits...\n');

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

  // Test 1: Main thread resourceLimits is empty object
  test('Main thread resourceLimits is empty object', () => {
    if (Object.keys(resourceLimits).length !== 0) {
      throw new Error(`Expected empty object, got ${JSON.stringify(resourceLimits)}`);
    }
  });

  // Test 2: resourceLimits is frozen
  test('resourceLimits is frozen', () => {
    if (!Object.isFrozen(resourceLimits)) {
      throw new Error('resourceLimits should be frozen');
    }
  });

  // Test 3: Create worker with resourceLimits - basic
  const testWithLimits = new Promise((resolve, reject) => {
    const worker = new Worker(__filename, {
      workerData: { test: 'limits' },
      resourceLimits: {
        maxOldGenerationSizeMb: 128,
        maxYoungGenerationSizeMb: 32,
      }
    });

    worker.on('message', (msg) => {
      if (msg.success) {
        console.log('✓ Worker created with resourceLimits');
        console.log('  Worker received limits:', JSON.stringify(msg.limits));
        passed++;
      } else {
        console.log('✗ Worker resourceLimits test failed:', msg.error);
        failed++;
      }
      resolve();
    });

    worker.on('error', (err) => {
      console.log('✗ Worker error:', err.message);
      failed++;
      resolve();
    });

    // Timeout
    setTimeout(() => {
      console.log('✗ Worker test timed out');
      failed++;
      resolve();
    }, 5000);
  });

  // Test 4: Worker.resourceLimits getter
  test('Worker.resourceLimits getter returns limits', () => {
    const worker = new Worker(__filename, {
      workerData: { test: 'getter' },
      resourceLimits: {
        maxOldGenerationSizeMb: 64,
      }
    });

    const limits = worker.resourceLimits;
    if (limits.maxOldGenerationSizeMb !== 64) {
      throw new Error(`Expected 64, got ${limits.maxOldGenerationSizeMb}`);
    }

    worker.terminate();
  });

  // Test 5: Worker without resourceLimits has empty limits
  test('Worker without resourceLimits has empty getter', () => {
    const worker = new Worker(__filename, {
      workerData: { test: 'no-limits' }
    });

    const limits = worker.resourceLimits;
    if (Object.keys(limits).length !== 0) {
      throw new Error(`Expected empty object, got ${JSON.stringify(limits)}`);
    }

    worker.terminate();
  });

  testWithLimits.then(() => {
    console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
    process.exit(failed > 0 ? 1 : 0);
  });

} else {
  // Worker thread
  const { parentPort, workerData, resourceLimits } = require('worker_threads');

  // Report back the resource limits we see
  parentPort.postMessage({
    success: true,
    limits: resourceLimits,
    workerData: workerData
  });
}
