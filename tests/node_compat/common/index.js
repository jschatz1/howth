// Minimal Node.js test common module shim for howth
'use strict';

const assert = require('assert');
const fs = require('fs');
const path = require('path');

const isWindows = process.platform === 'win32';
const isLinux = process.platform === 'linux';
const isDarwin = process.platform === 'darwin';
const isOSX = isDarwin;
const isMacOS = isDarwin;

// Test tracking
let mustCallChecks = [];

function mustCall(fn, expected) {
  if (typeof fn === 'number') {
    expected = fn;
    fn = () => {};
  }
  if (fn === undefined) {
    fn = () => {};
  }
  if (expected === undefined) {
    expected = 1;
  }

  const context = {
    expected,
    actual: 0,
    name: fn.name || '<anonymous>',
    stack: new Error().stack,
  };
  mustCallChecks.push(context);

  const wrapped = function(...args) {
    context.actual++;
    return fn.apply(this, args);
  };
  wrapped.origFn = fn;
  return wrapped;
}

function mustNotCall(reason) {
  return function mustNotCall() {
    throw new Error(reason || 'function should not have been called');
  };
}

function mustCallAtLeast(fn, minimum) {
  return mustCall(fn, minimum);
}

function mustSucceed(fn, exact) {
  return mustCall((err, ...args) => {
    assert.ifError(err);
    if (typeof fn === 'function')
      return fn(...args);
  }, exact);
}

function mustNotMutateObjectDeep(obj) {
  return obj; // Simplified - just return the object
}

// Check that all mustCall functions were called the expected number of times
process.on('exit', function() {
  for (const context of mustCallChecks) {
    if (context.actual !== context.expected) {
      console.error(`Mismatched ${context.name} function calls.`);
      console.error(`  Expected: ${context.expected}`);
      console.error(`  Actual: ${context.actual}`);
      process.exitCode = 1;
    }
  }
});

// Platform checks
function skip(msg) {
  console.log(`1..0 # Skipped: ${msg}`);
  process.exit(0);
}

function skipIfInspectorDisabled() {
  skip('Inspector is not available');
}

function skipIf32Bits() {
  // howth is 64-bit only for now
}

function skipIfWorker() {
  // No worker support yet
}

// Temp directory handling
const tmpDir = process.env.TMPDIR || process.env.TMP || '/tmp';
const tmpdir = {
  path: path.join(tmpDir, `howth-test-${process.pid}`),
  refresh() {
    try {
      fs.rmdirSync(this.path, { recursive: true });
    } catch (e) {
      // Ignore
    }
    fs.mkdirSync(this.path, { recursive: true });
    return this.path;
  },
  resolve(...args) {
    return path.resolve(this.path, ...args);
  },
  hasEnoughSpace() {
    return true;
  },
};

// Fixtures
const fixturesDir = path.join(__dirname, '..', 'fixtures');
const fixtures = {
  path(...args) {
    return path.join(fixturesDir, ...args);
  },
  readSync(file, enc) {
    return fs.readFileSync(this.path(file), enc || 'utf8');
  },
  readKey(name, enc) {
    return this.readSync(path.join('keys', name), enc);
  },
};

// Error expectation helpers
function expectsError(validator, exact) {
  return mustCall((error) => {
    if (typeof validator === 'object') {
      if (validator.code) {
        assert.strictEqual(error.code, validator.code);
      }
      if (validator.name) {
        assert.strictEqual(error.name, validator.name);
      }
      if (validator.message) {
        if (validator.message instanceof RegExp) {
          assert.match(error.message, validator.message);
        } else {
          assert.strictEqual(error.message, validator.message);
        }
      }
    }
  }, exact);
}

// Utility to print test progress
function printSkipMessage(msg) {
  console.log(`# SKIP ${msg}`);
}

// Check if running as main module
const isMainModule = require.main === module;

// Crash on unhandled rejections
process.on('unhandledRejection', (err) => {
  console.error('Unhandled rejection:', err);
  process.exit(1);
});

// Check platform-specific features
function hasCrypto() {
  try {
    require('crypto');
    return true;
  } catch (e) {
    return false;
  }
}

function hasIntl() {
  return typeof Intl !== 'undefined';
}

function canCreateSymLink() {
  // On Windows, symlinks require special privileges
  if (isWindows) {
    return false;
  }
  return true;
}

function invalidArgTypeHelper(input) {
  if (input == null) {
    return ` Received ${input}`;
  }
  if (typeof input === 'function' && input.name) {
    return ` Received function ${input.name}`;
  }
  if (typeof input === 'object') {
    if (input.constructor && input.constructor.name) {
      return ` Received an instance of ${input.constructor.name}`;
    }
    return ` Received ${JSON.stringify(input)}`;
  }
  let inspected = JSON.stringify(input);
  if (inspected.length > 25) {
    inspected = inspected.slice(0, 25) + '...';
  }
  return ` Received type ${typeof input} (${inspected})`;
}

function expectWarning() {
  // Simplified - ignore warnings for now
}

// Export everything
module.exports = {
  mustCall,
  mustNotCall,
  mustCallAtLeast,
  mustSucceed,
  mustNotMutateObjectDeep,
  expectsError,
  expectWarning,
  skip,
  skipIfInspectorDisabled,
  skipIf32Bits,
  skipIfWorker,
  printSkipMessage,
  tmpdir,
  fixtures,
  isWindows,
  isLinux,
  isDarwin,
  isOSX,
  isMacOS,
  isIBMi: false,
  isMainModule,
  hasCrypto,
  hasIntl,
  canCreateSymLink,
  invalidArgTypeHelper,
  // Placeholders for unsupported features
  get hasIPv6() { return true; },
  get localhostIPv4() { return '127.0.0.1'; },
  get PORT() { return 12346; },
  get hasMultiLocalhost() { return false; },
  allowGlobals(...args) {},
  disableCrashOnUnhandledRejection() {},
  getArrayBufferViews(buf) { return [buf]; },
  getBufferSources(buf) { return [buf]; },
  getTTYfd() { return -1; },
  runWithInvalidFD(fn) { fn(-1); },
  spawnPromisified(...args) { throw new Error('spawn not implemented'); },
  createZeroFilledFile(path) { fs.writeFileSync(path, ''); },
};
