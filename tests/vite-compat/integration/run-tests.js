#!/usr/bin/env node

/**
 * Vite Compatibility Test Runner
 *
 * Usage:
 *   node run-tests.js              # Run all tests (requires both vite and howth)
 *   HOWTH_ONLY=1 node run-tests.js # Run tests against howth only
 *   VITE_ONLY=1 node run-tests.js  # Run tests against vite only
 *   HOWTH_BIN=/path/to/howth node run-tests.js  # Use custom howth binary
 */

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_DIR = path.resolve(__dirname, '../fixtures/basic-app');

async function main() {
  console.log('=== Vite Compatibility Tests ===\n');

  // Check if fixture has node_modules
  const nodeModulesPath = path.join(FIXTURE_DIR, 'node_modules');
  if (!fs.existsSync(nodeModulesPath)) {
    console.log('Installing fixture dependencies...');
    await runCommand('npm', ['install'], { cwd: FIXTURE_DIR });
    console.log('');
  }

  // Check howth binary
  const howthBin = process.env.HOWTH_BIN || 'howth';
  if (process.env.HOWTH_ONLY !== '1' && process.env.VITE_ONLY !== '1') {
    try {
      await runCommand(howthBin, ['--version'], { silent: true });
      console.log(`Using howth: ${howthBin}`);
    } catch {
      console.error(`howth binary not found: ${howthBin}`);
      console.error('Set HOWTH_BIN environment variable or add howth to PATH');
      console.error('Or run with VITE_ONLY=1 to skip howth tests');
      process.exit(1);
    }
  }

  // Run tests
  const testFiles = fs.readdirSync(__dirname)
    .filter(f => f.endsWith('.test.js'))
    .map(f => path.join(__dirname, f));

  console.log(`\nRunning ${testFiles.length} test files...\n`);

  const result = await runCommand('node', [
    '--test',
    '--test-reporter', 'spec',
    ...testFiles,
  ], {
    cwd: __dirname,
    env: {
      ...process.env,
      HOWTH_BIN: howthBin,
    },
  });

  process.exit(result.code);
}

function runCommand(cmd, args, options = {}) {
  return new Promise((resolve) => {
    const proc = spawn(cmd, args, {
      cwd: options.cwd || process.cwd(),
      stdio: options.silent ? 'ignore' : 'inherit',
      env: options.env || process.env,
    });

    proc.on('close', (code) => {
      resolve({ code });
    });

    proc.on('error', (err) => {
      if (!options.silent) {
        console.error(`Error running ${cmd}:`, err.message);
      }
      resolve({ code: 1, error: err });
    });
  });
}

main().catch(console.error);
