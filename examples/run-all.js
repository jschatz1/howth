#!/usr/bin/env node
/**
 * Example Test Runner
 *
 * Runs all example apps to verify they work correctly with howth.
 * Used for end-to-end testing and CI validation.
 *
 * Run with Node.js: node examples/run-all.js
 * Run with howth:   howth run --native examples/run-all.js
 */

const { execSync, spawn } = require('child_process');
const path = require('path');
const fs = require('fs');

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
};

const EXAMPLES_DIR = path.dirname(process.argv[1] || __filename);

// Find howth binary: env var > release build > debug build > PATH
function findHowthBin() {
  if (process.env.HOWTH_BIN) return process.env.HOWTH_BIN;

  const releaseBin = path.join(EXAMPLES_DIR, '../target/release/howth');
  if (fs.existsSync(releaseBin)) return releaseBin;

  const debugBin = path.join(EXAMPLES_DIR, '../target/debug/howth');
  if (fs.existsSync(debugBin)) return debugBin;

  // Fall back to PATH
  try {
    return execSync('which howth', { encoding: 'utf8' }).trim();
  } catch {
    return null;
  }
}

const HOWTH_BIN = findHowthBin();

// Check if howth binary exists
if (!HOWTH_BIN || !fs.existsSync(HOWTH_BIN)) {
  console.error(`${c.red}Error: howth binary not found${c.reset}`);
  console.error('Run: cargo build --release --features native-runtime -p fastnode-cli');
  process.exit(1);
}

console.log(`\n${c.bold}${c.cyan}Howth Example Test Runner${c.reset}`);
console.log(`${c.dim}Using: ${HOWTH_BIN}${c.reset}\n`);

const results = {
  passed: [],
  failed: [],
};

// Test definitions
const tests = [
  {
    name: 'CLI Tool - Help',
    dir: 'cli-tool',
    script: 'cli.js',
    args: ['--help'],
    timeout: 5000,
    validate: (output, code) => code === 0 && output.includes('Howth CLI') && output.includes('Commands'),
  },
  {
    name: 'CLI Tool - Greet',
    dir: 'cli-tool',
    script: 'cli.js',
    args: ['greet', '--name', 'World'],
    timeout: 5000,
    validate: (output, code) => code === 0 && output.includes('Hello'),
  },
  {
    name: 'CLI Tool - Files',
    dir: 'cli-tool',
    script: 'cli.js',
    args: ['files', EXAMPLES_DIR],
    timeout: 5000,
    validate: (output, code) => code === 0 && output.includes('cli-tool'),
  },
  {
    name: 'File Processor - Analyze',
    dir: 'file-processor',
    script: 'processor.js',
    args: ['analyze', EXAMPLES_DIR],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Files by extension') && output.includes('.js'),
  },
  {
    name: 'File Processor - TODOs',
    dir: 'file-processor',
    script: 'processor.js',
    args: ['todos', EXAMPLES_DIR],
    timeout: 10000,
    validate: (output, code) => code === 0 && (output.includes('TODO') || output.includes('Total: 0 items')),
  },
  {
    name: 'Env Loader',
    dir: 'env-loader',
    script: 'index.js',
    args: [],
    timeout: 5000,
    validate: (output, code) => code === 0 && output.includes('Config loading demo completed'),
  },
  // HTTP Server tests (spawn and test)
  {
    name: 'HTTP Server - Startup',
    dir: 'http-server',
    script: 'server.js',
    args: [],
    timeout: 5000,
    server: true,
    port: 3100,
    // Server tests just check they don't crash on startup
    validate: (output, code) => code === null || code === 0 || output.includes('Server') || output.includes('listening'),
  },
  {
    name: 'TODO API - Startup',
    dir: 'todo-api',
    script: 'server.js',
    args: [],
    timeout: 5000,
    server: true,
    port: 3101,
    validate: (output, code) => code === null || code === 0 || output.includes('API') || output.includes('running'),
  },
  {
    name: 'Static Server - Startup',
    dir: 'static-server',
    script: 'server.js',
    args: [EXAMPLES_DIR],
    timeout: 5000,
    server: true,
    port: 3102,
    validate: (output, code) => code === null || code === 0 || output.includes('Server') || output.includes('Serving'),
  },
  // New examples
  {
    name: 'JSON Database',
    dir: 'json-db',
    script: 'db.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('JSON Database Demo') && output.includes('demo completed'),
  },
  {
    name: 'Test Runner',
    dir: 'test-runner',
    script: 'runner.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Running tests') && output.includes('All tests passed'),
  },
  {
    name: 'Proxy Server - Startup',
    dir: 'proxy-server',
    script: 'proxy.js',
    args: [],
    timeout: 5000,
    server: true,
    port: 3080,
    validate: (output, code) => code === null || code === 0 || output.includes('Proxy') || output.includes('routes'),
  },
  {
    name: 'Markdown Processor',
    dir: 'markdown',
    script: 'md.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Markdown Processor Demo') && output.includes('processing completed'),
  },
  {
    name: 'LRU Cache',
    dir: 'lru-cache',
    script: 'cache.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('LRU Cache Demo') && output.includes('demo completed'),
  },
  {
    name: 'Task Scheduler',
    dir: 'task-scheduler',
    script: 'scheduler.js',
    args: [],
    timeout: 60000, // Longer timeout - scheduler runs for 5 seconds but may have overhead
    validate: (output, code) => code === 0 && output.includes('Task Scheduler Demo') && output.includes('demo completed'),
  },
  // Frontend-focused examples
  {
    name: 'Dev Server - Startup',
    dir: 'dev-server',
    script: 'server.js',
    args: [],
    timeout: 5000,
    server: true,
    port: 3200,
    validate: (output, code) => code === null || code === 0 || output.includes('Dev Server') || output.includes('Local'),
  },
  {
    name: 'SSR App - Startup',
    dir: 'ssr-app',
    script: 'server.js',
    args: [],
    timeout: 5000,
    server: true,
    port: 3201,
    validate: (output, code) => code === null || code === 0 || output.includes('SSR') || output.includes('Pages'),
  },
  {
    name: 'Static Site Generator',
    dir: 'static-site',
    script: 'build.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Static Site Generator') && output.includes('Build completed'),
  },
  // API coverage examples
  {
    name: 'Crypto Utils',
    dir: 'crypto-utils',
    script: 'crypto.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Crypto Utilities Demo') && output.includes('demo completed'),
  },
  {
    name: 'Stream Pipeline',
    dir: 'stream-pipeline',
    script: 'streams.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Stream Processing Demo') && output.includes('demo completed'),
  },
  {
    name: 'Event System',
    dir: 'event-system',
    script: 'events.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Event System Demo') && output.includes('demo completed'),
  },
  {
    name: 'Buffer Operations',
    dir: 'buffer-ops',
    script: 'buffers.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Buffer Operations Demo') && output.includes('demo completed'),
  },
  {
    name: 'OS Info',
    dir: 'os-info',
    script: 'os.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('OS Information Demo') && output.includes('demo completed'),
  },
  {
    name: 'Util Helpers',
    dir: 'util-helpers',
    script: 'util.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Util Helpers Demo') && output.includes('demo completed'),
  },
  {
    name: 'WebSocket Chat - Startup',
    dir: 'websocket-chat',
    script: 'chat.js',
    args: [],
    timeout: 5000,
    server: true,
    port: 3300,
    validate: (output, code) => code === null || code === 0 || output.includes('WebSocket') || output.includes('Chat'),
  },
  {
    name: 'Rate Limiter - Startup',
    dir: 'rate-limiter',
    script: 'limiter.js',
    args: [],
    timeout: 5000,
    server: true,
    port: 3301,
    validate: (output, code) => code === null || code === 0 || output.includes('Rate Limiter') || output.includes('Token Bucket'),
  },
  {
    name: 'Job Queue',
    dir: 'job-queue',
    script: 'queue.js',
    args: [],
    timeout: 60000,
    validate: (output, code) => code === 0 && output.includes('Job Queue Demo') && output.includes('demo completed'),
  },
  {
    name: 'Template Engine',
    dir: 'template-engine',
    script: 'template.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Template Engine Demo') && output.includes('demo completed'),
  },
  {
    name: 'Validator',
    dir: 'validator',
    script: 'validate.js',
    args: [],
    timeout: 10000,
    validate: (output, code) => code === 0 && output.includes('Data Validator Demo') && output.includes('demo completed'),
  },
];

// Run a single test
async function runTest(test) {
  const scriptPath = path.join(EXAMPLES_DIR, test.dir, test.script);

  if (!fs.existsSync(scriptPath)) {
    return { success: false, error: `Script not found: ${scriptPath}` };
  }

  return new Promise((resolve) => {
    // Arguments after script must be passed with -- separator
    const args = ['run', '--native', scriptPath];
    if (test.args.length > 0) {
      args.push('--', ...test.args);
    }
    let output = '';
    let error = '';

    const proc = spawn(HOWTH_BIN, args, {
      cwd: path.join(EXAMPLES_DIR, test.dir),
      // Don't use spawn timeout - we have our own timeout handler
      env: { ...process.env, PORT: test.port?.toString() },
    });

    proc.stdout.on('data', (data) => {
      output += data.toString();
    });

    proc.stderr.on('data', (data) => {
      error += data.toString();
    });

    // For server tests, kill after short delay and check output
    if (test.server) {
      setTimeout(() => {
        // Give a bit more time to capture output before killing
        setTimeout(() => {
          proc.kill('SIGTERM');
        }, 500);
      }, 1500);
    }

    proc.on('close', (code) => {
      // For server tests, we expect to kill them (code will be null or non-zero)
      const combined = output + error;

      // Pass code to validator for server tests
      if (test.validate(combined, code)) {
        resolve({ success: true, output: combined });
      } else {
        resolve({
          success: false,
          error: error || output || `Validation failed (code ${code}). Output: ${combined.slice(0, 200)}`,
          code,
        });
      }
    });

    proc.on('error', (err) => {
      resolve({ success: false, error: err.message });
    });

    // Timeout handler
    setTimeout(() => {
      if (!proc.killed) {
        proc.kill('SIGKILL');
        resolve({ success: false, error: 'Test timed out' });
      }
    }, test.timeout + 1000);
  });
}

// Run all tests
async function runAllTests() {
  console.log(`${c.bold}Running ${tests.length} tests...${c.reset}\n`);

  for (const test of tests) {
    process.stdout.write(`  ${test.name}... `);

    try {
      const result = await runTest(test);

      if (result.success) {
        console.log(`${c.green}✓ PASS${c.reset}`);
        results.passed.push(test.name);
      } else {
        console.log(`${c.red}✗ FAIL${c.reset}`);
        console.log(`    ${c.dim}${result.error?.slice(0, 100)}${c.reset}`);
        results.failed.push({ name: test.name, error: result.error });
      }
    } catch (err) {
      console.log(`${c.red}✗ ERROR${c.reset}`);
      console.log(`    ${c.dim}${err.message}${c.reset}`);
      results.failed.push({ name: test.name, error: err.message });
    }
  }

  // Summary
  console.log(`\n${c.bold}${'═'.repeat(50)}${c.reset}`);
  console.log(`${c.bold}Summary${c.reset}`);
  console.log(`${'═'.repeat(50)}`);
  console.log(`  ${c.green}Passed:${c.reset} ${results.passed.length}`);
  console.log(`  ${c.red}Failed:${c.reset} ${results.failed.length}`);
  console.log(`  ${c.cyan}Total:${c.reset}  ${tests.length}`);

  const passRate = Math.round((results.passed.length / tests.length) * 100);
  console.log(`\n  ${c.bold}Pass rate: ${passRate}%${c.reset}`);

  if (results.failed.length > 0) {
    console.log(`\n${c.red}Failed tests:${c.reset}`);
    for (const { name, error } of results.failed) {
      console.log(`  ${c.red}✗${c.reset} ${name}`);
      console.log(`    ${c.dim}${error?.slice(0, 80)}${c.reset}`);
    }
    process.exit(1);
  } else {
    console.log(`\n${c.green}${c.bold}All examples passed!${c.reset}\n`);
  }
}

runAllTests();
