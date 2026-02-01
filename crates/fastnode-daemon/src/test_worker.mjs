// Warm Node worker for howth test runner.
// Reads newline-delimited JSON messages from stdin, runs tests via node:test,
// and writes JSON results to stdout.
//
// Uses isolation: 'none' so tests run in-process (no child process spawning).
// Requires stdin.unref()/ref() around run() calls so node:test's event loop
// drain detection works correctly.
//
// Message format (stdin):
//   { "id": "...", "files": [{ "path": "orig.test.ts", "code": "..." }] }
//
// Result format (stdout):
//   { "id": "...", "ok": true, "total": N, "passed": N, "failed": N,
//     "skipped": N, "duration_ms": N, "tests": [...], "diagnostics": "" }

import { run } from 'node:test';
import { writeFileSync, unlinkSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const TEMP_PREFIX = join(tmpdir(), 'howth-test-worker');

// Ensure temp dir exists
try { mkdirSync(TEMP_PREFIX, { recursive: true }); } catch {}

let messageQueue = [];
let processing = false;

process.stdin.setEncoding('utf8');
let buffer = '';

process.stdin.on('data', (chunk) => {
  buffer += chunk;
  let nl;
  while ((nl = buffer.indexOf('\n')) !== -1) {
    const line = buffer.slice(0, nl);
    buffer = buffer.slice(nl + 1);
    if (line.trim()) {
      messageQueue.push(JSON.parse(line));
      drainQueue();
    }
  }
});

process.stdin.on('end', () => {
  process.exit(0);
});

async function drainQueue() {
  if (processing) return;
  while (messageQueue.length > 0) {
    processing = true;
    const msg = messageQueue.shift();

    // Unref stdin so node:test run() with isolation: 'none' can detect
    // event loop drain and end the test stream properly.
    process.stdin.unref();

    try {
      await handleMessage(msg);
    } catch (err) {
      process.stderr.write(`worker error: ${err.message}\n`);
    }

    // Re-ref stdin so we keep listening for more messages
    process.stdin.ref();
    processing = false;
  }
}

async function handleMessage(msg) {
  const { id, files } = msg;

  // Write transpiled code to temp files
  const tempFiles = [];
  for (let i = 0; i < files.length; i++) {
    const f = files[i];
    const tmp = join(TEMP_PREFIX, `${process.pid}-${id}-${i}.cjs`);
    writeFileSync(tmp, f.code);
    tempFiles.push(tmp);
  }

  const start = performance.now();
  let total = 0;
  let passed = 0;
  let failed = 0;
  let skipped = 0;
  const tests = [];
  let diagnostics = '';

  try {
    const stream = run({ files: tempFiles, concurrency: true, isolation: 'none' });

    for await (const event of stream) {
      if (event.type === 'test:pass') {
        // Only count leaf tests (not suites)
        if (event.data.details !== undefined) {
          passed++;
          total++;
          tests.push({
            name: event.data.name,
            file: event.data.file || '',
            status: 'pass',
            duration_ms: event.data.details?.duration_ms ?? 0,
          });
        }
      } else if (event.type === 'test:fail') {
        if (event.data.details !== undefined) {
          failed++;
          total++;
          tests.push({
            name: event.data.name,
            file: event.data.file || '',
            status: 'fail',
            duration_ms: event.data.details?.duration_ms ?? 0,
            error: event.data.details?.error
              ? String(event.data.details.error.message || event.data.details.error)
              : undefined,
          });
        }
      } else if (event.type === 'test:skip') {
        skipped++;
        total++;
        tests.push({
          name: event.data.name,
          file: event.data.file || '',
          status: 'skip',
          duration_ms: 0,
        });
      } else if (event.type === 'test:diagnostic') {
        if (event.data.message) {
          diagnostics += event.data.message + '\n';
        }
      }
    }
  } catch (err) {
    diagnostics += `runner error: ${err.message}\n`;
  } finally {
    // Clean up temp files
    for (const f of tempFiles) {
      try { unlinkSync(f); } catch {}
    }
  }

  const duration_ms = performance.now() - start;
  const ok = failed === 0;
  const result = JSON.stringify({
    id, ok, total, passed, failed, skipped, duration_ms, tests, diagnostics,
  }) + '\n';
  process.stdout.write(result);
}
