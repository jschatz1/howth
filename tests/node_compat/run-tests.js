#!/usr/bin/env node
// Node.js compatibility test runner for howth
'use strict';

const fs = require('fs');
const path = require('path');
const { execSync, spawn } = require('child_process');

const NODEJS_REPO = 'https://raw.githubusercontent.com/nodejs/node/main';
const TESTS_DIR = __dirname;
const PARALLEL_DIR = path.join(TESTS_DIR, 'parallel');

// List of tests to run (relative to test/parallel/)
const TESTS = [
  // Buffer tests
  'test-buffer-basic.js',

  // URL tests
  'test-url-basic.js',

  // Process tests
  'test-process-basic.js',

  // Events tests
  'test-events-basic.js',

  // Util tests
  'test-util-basic.js',

  // Path tests
  'test-path.js',
  'test-path-parse-format.js',
  'test-path-dirname.js',
  'test-path-basename.js',
  'test-path-extname.js',
  'test-path-join.js',
  'test-path-relative.js',
  'test-path-resolve.js',
  'test-path-isabsolute.js',
  'test-path-normalize.js',

  // FS tests (sync operations)
  'test-fs-exists.js',
  'test-fs-stat.js',
  'test-fs-mkdir.js',
  'test-fs-readdir.js',
  'test-fs-readfile.js',
  'test-fs-writefile.js',
  'test-fs-access.js',
  'test-fs-appendfile.js',
  'test-fs-copyfile.js',
  'test-fs-rename.js',
  'test-fs-unlink.js',
  'test-fs-realpath.js',
];

// Tests that require external dependencies (skip these)
const SKIP_TESTS = new Set([
  // Tests that require Node.js internal modules
  'test-fs-access.js',       // requires internal/test/binding
  'test-fs-copyfile.js',     // requires internal/test/binding

  // Tests that require worker_threads module
  'test-fs-mkdir.js',        // requires worker_threads
  'test-fs-realpath.js',     // requires worker_threads

  // Tests that require child_process module (now available)
  // 'test-path-resolve.js',    // child_process now implemented

  // Tests that require fstat on stdin/stdout/stderr (not supported in Deno)
  'test-fs-stat.js',         // fstat on fd 0 (stdin)

  // Tests that require CVE-2024-36139 security fixes (complex path traversal prevention)
  'test-path-normalize.js',  // CVE-2024-36139 Windows path traversal
  'test-path-join.js',       // CVE-2024-36139 Windows path traversal

  // Tests with directory structure assumptions (patched for howth)
  // 'test-path-dirname.js',    // patched to work with tests/node_compat/parallel
]);

// Download a test file from Node.js repo
async function downloadTest(testName) {
  const url = `${NODEJS_REPO}/test/parallel/${testName}`;
  const destPath = path.join(PARALLEL_DIR, testName);

  if (fs.existsSync(destPath)) {
    return true; // Already downloaded
  }

  try {
    console.log(`  Downloading ${testName}...`);
    const response = await fetch(url);
    if (!response.ok) {
      console.log(`  ⚠ Could not download ${testName} (${response.status})`);
      return false;
    }
    let content = await response.text();

    // Patch require paths to work with our directory structure
    content = content.replace(
      /require\(['"]\.\.\/common['"]\)/g,
      "require('../common')"
    );
    content = content.replace(
      /require\(['"]\.\.\/common\/tmpdir['"]\)/g,
      "require('../common/tmpdir')"
    );
    content = content.replace(
      /require\(['"]\.\.\/\.\.\/test\/common\/tmpdir['"]\)/g,
      "require('../common/tmpdir')"
    );
    content = content.replace(
      /require\(['"]\.\.\/common\/fixtures['"]\)/g,
      "require('../common').fixtures"
    );

    fs.writeFileSync(destPath, content);
    return true;
  } catch (e) {
    console.log(`  ⚠ Failed to download ${testName}: ${e.message}`);
    return false;
  }
}

// Run a single test
function runTest(testPath, howthBin) {
  return new Promise((resolve) => {
    const startTime = Date.now();
    // Use --native flag to use howth's native V8 runtime instead of Node.js subprocess
    const proc = spawn(howthBin, ['run', '--native', testPath], {
      stdio: ['ignore', 'pipe', 'pipe'],
      timeout: 30000,
    });

    let stdout = '';
    let stderr = '';

    proc.stdout.on('data', (data) => { stdout += data; });
    proc.stderr.on('data', (data) => { stderr += data; });

    proc.on('close', (code) => {
      const duration = Date.now() - startTime;
      resolve({
        success: code === 0,
        code,
        stdout,
        stderr,
        duration,
      });
    });

    proc.on('error', (err) => {
      resolve({
        success: false,
        code: -1,
        stdout,
        stderr: err.message,
        duration: Date.now() - startTime,
      });
    });
  });
}

async function main() {
  console.log('Node.js Compatibility Test Runner');
  console.log('==================================\n');

  // Ensure directories exist
  fs.mkdirSync(PARALLEL_DIR, { recursive: true });

  // Find howth binary
  const howthBin = process.env.HOWTH_BIN ||
    path.join(__dirname, '../../target/debug/howth');

  if (!fs.existsSync(howthBin)) {
    console.error(`Error: howth binary not found at ${howthBin}`);
    console.error('Run: cargo build --features native-runtime -p fastnode-cli');
    process.exit(1);
  }

  console.log(`Using howth: ${howthBin}\n`);

  // Download tests
  console.log('Downloading tests from Node.js repo...');
  const downloadedTests = [];
  for (const test of TESTS) {
    if (await downloadTest(test)) {
      downloadedTests.push(test);
    }
  }
  console.log(`Downloaded ${downloadedTests.length}/${TESTS.length} tests\n`);

  // Run tests
  console.log('Running tests...\n');

  const results = {
    passed: [],
    failed: [],
    skipped: [],
  };

  for (const test of downloadedTests) {
    if (SKIP_TESTS.has(test)) {
      results.skipped.push(test);
      console.log(`⏭  ${test} (skipped)`);
      continue;
    }

    const testPath = path.join(PARALLEL_DIR, test);
    const result = await runTest(testPath, howthBin);

    if (result.success) {
      results.passed.push(test);
      console.log(`✓  ${test} (${result.duration}ms)`);
    } else {
      results.failed.push({ test, ...result });
      console.log(`✗  ${test} (${result.duration}ms)`);
      if (process.env.VERBOSE) {
        console.log(`   stdout: ${result.stdout.slice(0, 200)}`);
        console.log(`   stderr: ${result.stderr.slice(0, 200)}`);
      }
    }
  }

  // Summary
  console.log('\n==================================');
  console.log('Summary');
  console.log('==================================');
  console.log(`Passed:  ${results.passed.length}`);
  console.log(`Failed:  ${results.failed.length}`);
  console.log(`Skipped: ${results.skipped.length}`);
  console.log(`Total:   ${downloadedTests.length}`);

  const passRate = downloadedTests.length > 0
    ? Math.round((results.passed.length / downloadedTests.length) * 100)
    : 0;
  console.log(`\nPass rate: ${passRate}%`);

  if (results.failed.length > 0) {
    console.log('\nFailed tests:');
    for (const { test, stderr } of results.failed) {
      console.log(`  - ${test}`);
      if (stderr) {
        const firstLine = stderr.split('\n')[0].slice(0, 80);
        console.log(`    Error: ${firstLine}`);
      }
    }
  }

  process.exit(results.failed.length > 0 ? 1 : 0);
}

main().catch(console.error);
