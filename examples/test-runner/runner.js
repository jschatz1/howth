/**
 * Test Runner Example
 *
 * A minimal test framework with:
 * - describe/it syntax (like Mocha/Jest)
 * - Assertions
 * - Async test support
 * - Before/after hooks
 * - Test filtering
 * - Colored output with timing
 *
 * Run: howth run --native examples/test-runner/runner.js
 */

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

// Test state
const suites = [];
let currentSuite = null;
const results = { passed: 0, failed: 0, skipped: 0, total: 0 };

/**
 * Assertion library
 */
const assert = {
  equal(actual, expected, message) {
    if (actual !== expected) {
      throw new AssertionError(
        message || `Expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`
      );
    }
  },

  deepEqual(actual, expected, message) {
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      throw new AssertionError(
        message || `Expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`
      );
    }
  },

  notEqual(actual, expected, message) {
    if (actual === expected) {
      throw new AssertionError(message || `Expected values to be different`);
    }
  },

  ok(value, message) {
    if (!value) {
      throw new AssertionError(message || `Expected truthy value, got ${value}`);
    }
  },

  notOk(value, message) {
    if (value) {
      throw new AssertionError(message || `Expected falsy value, got ${value}`);
    }
  },

  throws(fn, expectedError, message) {
    let threw = false;
    let error;
    try {
      fn();
    } catch (e) {
      threw = true;
      error = e;
    }
    if (!threw) {
      throw new AssertionError(message || 'Expected function to throw');
    }
    if (expectedError && !error.message.includes(expectedError)) {
      throw new AssertionError(
        message || `Expected error containing "${expectedError}", got "${error.message}"`
      );
    }
  },

  async rejects(promise, expectedError, message) {
    let threw = false;
    let error;
    try {
      await promise;
    } catch (e) {
      threw = true;
      error = e;
    }
    if (!threw) {
      throw new AssertionError(message || 'Expected promise to reject');
    }
    if (expectedError && !error.message.includes(expectedError)) {
      throw new AssertionError(
        message || `Expected error containing "${expectedError}", got "${error.message}"`
      );
    }
  },

  isType(value, type, message) {
    const actualType = Array.isArray(value) ? 'array' : typeof value;
    if (actualType !== type) {
      throw new AssertionError(message || `Expected type ${type}, got ${actualType}`);
    }
  },

  includes(collection, item, message) {
    const has = Array.isArray(collection)
      ? collection.includes(item)
      : collection.indexOf(item) !== -1;
    if (!has) {
      throw new AssertionError(message || `Expected collection to include ${JSON.stringify(item)}`);
    }
  },

  match(value, regex, message) {
    if (!regex.test(value)) {
      throw new AssertionError(message || `Expected "${value}" to match ${regex}`);
    }
  },
};

class AssertionError extends Error {
  constructor(message) {
    super(message);
    this.name = 'AssertionError';
  }
}

/**
 * Test Suite
 */
class Suite {
  constructor(name, parent = null) {
    this.name = name;
    this.parent = parent;
    this.tests = [];
    this.suites = [];
    this.beforeAll = [];
    this.afterAll = [];
    this.beforeEach = [];
    this.afterEach = [];
  }

  fullName() {
    if (this.parent) {
      return `${this.parent.fullName()} > ${this.name}`;
    }
    return this.name;
  }
}

/**
 * Test Case
 */
class Test {
  constructor(name, fn, suite) {
    this.name = name;
    this.fn = fn;
    this.suite = suite;
    this.skip = false;
    this.only = false;
  }

  fullName() {
    return `${this.suite.fullName()} > ${this.name}`;
  }
}

// DSL functions
function describe(name, fn) {
  const suite = new Suite(name, currentSuite);
  if (currentSuite) {
    currentSuite.suites.push(suite);
  } else {
    suites.push(suite);
  }
  const prevSuite = currentSuite;
  currentSuite = suite;
  fn();
  currentSuite = prevSuite;
}

function it(name, fn) {
  if (!currentSuite) {
    throw new Error('it() must be called inside describe()');
  }
  currentSuite.tests.push(new Test(name, fn, currentSuite));
}

// Skip and only variants
it.skip = function(name, fn) {
  if (!currentSuite) throw new Error('it.skip() must be called inside describe()');
  const test = new Test(name, fn, currentSuite);
  test.skip = true;
  currentSuite.tests.push(test);
};

it.only = function(name, fn) {
  if (!currentSuite) throw new Error('it.only() must be called inside describe()');
  const test = new Test(name, fn, currentSuite);
  test.only = true;
  currentSuite.tests.push(test);
};

// Hooks
function beforeAll(fn) {
  if (!currentSuite) throw new Error('beforeAll() must be called inside describe()');
  currentSuite.beforeAll.push(fn);
}

function afterAll(fn) {
  if (!currentSuite) throw new Error('afterAll() must be called inside describe()');
  currentSuite.afterAll.push(fn);
}

function beforeEach(fn) {
  if (!currentSuite) throw new Error('beforeEach() must be called inside describe()');
  currentSuite.beforeEach.push(fn);
}

function afterEach(fn) {
  if (!currentSuite) throw new Error('afterEach() must be called inside describe()');
  currentSuite.afterEach.push(fn);
}

// Check if any tests have .only
function hasOnlyTests(suites) {
  for (const suite of suites) {
    if (suite.tests.some(t => t.only)) return true;
    if (hasOnlyTests(suite.suites)) return true;
  }
  return false;
}

// Run a single test
async function runTest(test, beforeEachHooks, afterEachHooks) {
  results.total++;

  if (test.skip) {
    results.skipped++;
    console.log(`  ${c.yellow}○${c.reset} ${c.dim}${test.name} (skipped)${c.reset}`);
    return;
  }

  const start = Date.now();

  try {
    // Run beforeEach hooks
    for (const hook of beforeEachHooks) {
      await hook();
    }

    // Run test
    await test.fn();

    // Run afterEach hooks
    for (const hook of afterEachHooks) {
      await hook();
    }

    const duration = Date.now() - start;
    results.passed++;
    console.log(`  ${c.green}✓${c.reset} ${test.name} ${c.dim}(${duration}ms)${c.reset}`);
  } catch (error) {
    const duration = Date.now() - start;
    results.failed++;
    console.log(`  ${c.red}✗${c.reset} ${test.name} ${c.dim}(${duration}ms)${c.reset}`);
    console.log(`    ${c.red}${error.message}${c.reset}`);
    if (error.stack && !error.name.includes('Assertion')) {
      console.log(`    ${c.dim}${error.stack.split('\n').slice(1, 3).join('\n    ')}${c.reset}`);
    }
  }
}

// Run a suite
async function runSuite(suite, depth = 0, beforeEachHooks = [], afterEachHooks = [], onlyMode = false) {
  const indent = '  '.repeat(depth);
  console.log(`${indent}${c.bold}${suite.name}${c.reset}`);

  // Collect hooks
  const allBeforeEach = [...beforeEachHooks, ...suite.beforeEach];
  const allAfterEach = [...suite.afterEach, ...afterEachHooks];

  // Run beforeAll hooks
  for (const hook of suite.beforeAll) {
    await hook();
  }

  // Run tests
  for (const test of suite.tests) {
    if (onlyMode && !test.only) {
      results.total++;
      results.skipped++;
      continue;
    }
    await runTest(test, allBeforeEach, allAfterEach);
  }

  // Run nested suites
  for (const nested of suite.suites) {
    await runSuite(nested, depth + 1, allBeforeEach, allAfterEach, onlyMode);
  }

  // Run afterAll hooks
  for (const hook of suite.afterAll) {
    await hook();
  }
}

// Run all tests
async function run() {
  console.log(`\n${c.bold}${c.cyan}Running tests...${c.reset}\n`);
  const start = Date.now();
  const onlyMode = hasOnlyTests(suites);

  for (const suite of suites) {
    await runSuite(suite, 0, [], [], onlyMode);
  }

  const duration = Date.now() - start;

  // Summary
  console.log(`\n${c.bold}${'─'.repeat(50)}${c.reset}`);
  console.log(`${c.bold}Results${c.reset}`);
  console.log(`${'─'.repeat(50)}`);
  console.log(`  ${c.green}Passed:${c.reset}  ${results.passed}`);
  console.log(`  ${c.red}Failed:${c.reset}  ${results.failed}`);
  console.log(`  ${c.yellow}Skipped:${c.reset} ${results.skipped}`);
  console.log(`  Total:   ${results.total}`);
  console.log(`  Time:    ${duration}ms`);

  if (results.failed > 0) {
    console.log(`\n${c.red}${c.bold}Tests failed!${c.reset}\n`);
    process.exit(1);
  } else {
    console.log(`\n${c.green}${c.bold}All tests passed!${c.reset}\n`);
  }
}

// Export for module use
if (typeof module !== 'undefined') {
  module.exports = { describe, it, beforeAll, afterAll, beforeEach, afterEach, assert, run };
}

// ============================================
// Demo Tests
// ============================================

describe('Math operations', () => {
  describe('addition', () => {
    it('adds positive numbers', () => {
      assert.equal(1 + 2, 3);
    });

    it('adds negative numbers', () => {
      assert.equal(-1 + -2, -3);
    });

    it('adds zero', () => {
      assert.equal(5 + 0, 5);
    });
  });

  describe('multiplication', () => {
    it('multiplies positive numbers', () => {
      assert.equal(3 * 4, 12);
    });

    it('handles zero', () => {
      assert.equal(5 * 0, 0);
    });
  });
});

describe('Array operations', () => {
  let arr;

  beforeEach(() => {
    arr = [1, 2, 3];
  });

  it('pushes elements', () => {
    arr.push(4);
    assert.equal(arr.length, 4);
    assert.deepEqual(arr, [1, 2, 3, 4]);
  });

  it('pops elements', () => {
    const popped = arr.pop();
    assert.equal(popped, 3);
    assert.equal(arr.length, 2);
  });

  it('finds elements', () => {
    assert.ok(arr.includes(2));
    assert.notOk(arr.includes(5));
  });

  it('maps elements', () => {
    const doubled = arr.map(x => x * 2);
    assert.deepEqual(doubled, [2, 4, 6]);
  });

  it.skip('this test is skipped', () => {
    assert.ok(false);
  });
});

describe('String operations', () => {
  it('concatenates strings', () => {
    assert.equal('hello' + ' ' + 'world', 'hello world');
  });

  it('matches regex', () => {
    assert.match('hello@example.com', /^\w+@\w+\.\w+$/);
  });

  it('includes substrings', () => {
    assert.includes('hello world', 'world');
  });
});

describe('Async operations', () => {
  it('handles promises', async () => {
    const result = await Promise.resolve(42);
    assert.equal(result, 42);
  });

  it('handles async/await', async () => {
    const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
    const start = Date.now();
    await delay(50);
    const elapsed = Date.now() - start;
    assert.ok(elapsed >= 45, 'Should wait at least 45ms');
  });

  it('catches rejected promises', async () => {
    await assert.rejects(
      Promise.reject(new Error('test error')),
      'test error'
    );
  });
});

describe('Type assertions', () => {
  it('checks types', () => {
    assert.isType('hello', 'string');
    assert.isType(42, 'number');
    assert.isType(true, 'boolean');
    assert.isType([1, 2], 'array');
    assert.isType({ a: 1 }, 'object');
  });
});

describe('Error handling', () => {
  it('catches thrown errors', () => {
    assert.throws(() => {
      throw new Error('test error');
    }, 'test error');
  });

  it('catches any error', () => {
    assert.throws(() => {
      JSON.parse('invalid json');
    });
  });
});

// Run tests
run();
