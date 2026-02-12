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
import { writeFileSync, unlinkSync, mkdirSync, readdirSync } from 'node:fs';
import { join, dirname, basename, extname } from 'node:path';
import { tmpdir } from 'node:os';

// Write a howth:mocha shim that wraps node:test with .timeout() chaining
const SHIM_DIR = join(tmpdir(), 'howth-test-worker');
try { mkdirSync(SHIM_DIR, { recursive: true }); } catch {}
const SHIM_PATH = join(SHIM_DIR, 'howth-mocha-shim.mjs');
writeFileSync(SHIM_PATH, `
import { describe as _describe, it as _it, before, after, beforeEach, afterEach } from 'node:test';
function chainable(result) {
  const c = { timeout() { return c; }, slow() { return c; }, retries() { return c; } };
  if (result && typeof result.then === 'function') { c.then = result.then.bind(result); c.catch = result.catch.bind(result); }
  return c;
}
const mochaCtx = { timeout() { return mochaCtx; }, slow() { return mochaCtx; }, retries() { return mochaCtx; }, skip() {} };
function bindCtx(fn) { if (!fn) return fn; return function(...a) { return fn.call(mochaCtx, ...a); }; }
function describe(name, fn) { return chainable(_describe(name, bindCtx(fn))); }
describe.only = function(name, fn) { return chainable(_describe(name, { only: true }, bindCtx(fn))); };
describe.skip = function(name, fn) { return chainable(_describe(name, { skip: true }, bindCtx(fn))); };
const context = describe;
function it(name, fn) { return chainable(_it(name, bindCtx(fn))); }
it.only = function(name, fn) { return chainable(_it(name, { only: true }, bindCtx(fn))); };
it.skip = function(name, fn) { return chainable(_it(name, { skip: true }, bindCtx(fn))); };
const specify = it;
export { describe, context, it, specify, before, after, beforeEach, afterEach };
export default describe;
`);

// Reserve stdout exclusively for the JSON protocol.
// Discard any stdout writes from test code (e.g. loggers) so they
// don't corrupt the protocol stream or fill the stderr pipe buffer
// (which would deadlock the process on macOS's 16KB pipe limit).
const _stdoutWrite = process.stdout.write.bind(process.stdout);
process.stdout.write = function(chunk, encoding, callback) {
  if (typeof callback === 'function') callback();
  return true;
};

// Track all temp files for cleanup on exit (handles timeout/kill scenarios)
const allTempFiles = new Set();
process.on('exit', () => {
  for (const f of allTempFiles) {
    try { unlinkSync(f); } catch {}
  }
});
// Also handle SIGTERM (daemon kill_on_drop)
process.on('SIGTERM', () => {
  for (const f of allTempFiles) {
    try { unlinkSync(f); } catch {}
  }
  process.exit(0);
});

// Clean up stale temp files from previous worker instances
function cleanupStaleFiles(dir) {
  try {
    for (const entry of readdirSync(dir)) {
      if (entry.startsWith('.howth-test-') && !entry.startsWith(`.howth-test-${process.pid}-`)) {
        try { unlinkSync(join(dir, entry)); } catch {}
      }
    }
  } catch {}
}

// Format an error object into a rich string with message, expected/actual, and stack.
function formatError(err) {
  if (!err) return undefined;
  let msg = String(err.message || err);
  if (err.expected !== undefined && err.actual !== undefined) {
    msg += `\nexpected: ${JSON.stringify(err.expected)}\nactual:   ${JSON.stringify(err.actual)}`;
  }
  if (err.stack) {
    // Extract file locations from stack (skip the first line which is the message)
    const lines = String(err.stack).split('\n');
    const stackLines = lines.filter(l => l.trimStart().startsWith('at '));
    if (stackLines.length > 0) {
      msg += '\n' + stackLines.slice(0, 5).join('\n');
    }
  }
  return msg;
}

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
  const { id, files, force_exit } = msg;

  // Write transpiled code to temp files next to originals so that
  // Node's module resolution finds node_modules and relative imports work.
  const tempFiles = [];
  const seenDirs = new Set();
  for (let i = 0; i < files.length; i++) {
    const f = files[i];
    const dir = dirname(f.path);
    // Strip .test/.spec suffix so temp files don't match node:test discovery
    // patterns (*.test.mjs). Without this, stale temp files from timed-out runs
    // get picked up by node:test isolation:'none' on subsequent runs.
    const name = basename(f.path, extname(f.path)).replace(/\.(test|spec)$/, '');
    const ext = f.path.endsWith('.cjs') || f.path.endsWith('.cts') ? '.cjs' : '.mjs';
    const tmp = join(dir, `.howth-test-${process.pid}-${id}-${name}${ext}`);
    writeFileSync(tmp, f.code);
    tempFiles.push(tmp);
    allTempFiles.add(tmp);
    // Clean up stale temp files from previous runs in each directory
    if (!seenDirs.has(dir)) {
      seenDirs.add(dir);
      cleanupStaleFiles(dir);
    }
  }

  const start = performance.now();
  let total = 0;
  let passed = 0;
  let failed = 0;
  let skipped = 0;
  const tests = [];
  let diagnostics = '';

  try {
    const stream = run({ files: tempFiles, concurrency: false, isolation: 'none' });

    // With isolation:'none', all test files run in-process. If test code leaves
    // open handles (DB connections, timers, gRPC channels), the event loop never
    // drains and `for await` hangs forever. Use an unreffed interval to detect
    // idle and destroy the stream to break out.
    // With --exit (force_exit), use a short 500ms idle timeout so we exit quickly
    // after tests finish. Without it, use 5s to be safe.
    const idleTimeout = force_exit ? 500 : 5000;
    let lastEventTime = performance.now();
    const idleCheck = setInterval(() => {
      if (performance.now() - lastEventTime > idleTimeout) {
        clearInterval(idleCheck);
        try { stream.destroy(); } catch {}
      }
    }, 200);
    idleCheck.unref();

    try {
      for await (const event of stream) {
        lastEventTime = performance.now();
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
              error: formatError(event.data.details?.error),
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
          // Filter out node:test summary lines (tests N, suites N, etc.)
          // — the CLI already prints its own summary from pass/fail counts.
          const msg = event.data.message;
          if (msg && !/^(tests|suites|pass|fail|cancelled|skipped|todo|duration_ms) /.test(msg)) {
            diagnostics += msg + '\n';
          }
        }
      }
    } catch (err) {
      // ERR_STREAM_PREMATURE_CLOSE is expected when idle check destroys the
      // stream (open handles from test code prevented normal stream end).
      if (err.code !== 'ERR_STREAM_PREMATURE_CLOSE') {
        diagnostics += `runner error: ${err.message}\n`;
      }
    }
    clearInterval(idleCheck);
  } catch (err) {
    diagnostics += `runner error: ${err.message}\n`;
  } finally {
    // Clean up temp files
    for (const f of tempFiles) {
      try { unlinkSync(f); } catch {}
      allTempFiles.delete(f);
    }
  }

  const duration_ms = performance.now() - start;
  const ok = failed === 0;
  const result = JSON.stringify({
    id, ok, total, passed, failed, skipped, duration_ms, tests, diagnostics,
  }) + '\n';
  _stdoutWrite(result);
}
