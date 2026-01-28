/**
 * Util Helpers Example
 *
 * Demonstrates Node.js util module:
 * - promisify
 * - callbackify
 * - inspect
 * - format
 * - types
 * - deprecate
 * - inherits
 *
 * Run: howth run --native examples/util-helpers/util.js
 */

const util = require('util');
const fs = require('fs');
const path = require('path');

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  red: '\x1b[31m',
  dim: '\x1b[2m',
};

console.log(`\n${c.bold}${c.cyan}Util Helpers Demo${c.reset}\n`);

// 1. util.format
console.log(`${c.bold}1. util.format${c.reset}`);
console.log(`  format('%s', 'hello'):        "${util.format('%s', 'hello')}"`);
console.log(`  format('%d + %d = %d', 1,2,3): "${util.format('%d + %d = %d', 1, 2, 3)}"`);
console.log(`  format('%j', {a:1}):          "${util.format('%j', { a: 1 })}"`);
console.log(`  format('%o', {a:1}):          "${util.format('%o', { a: 1 })}"`);
console.log(`  format('%%'):                 "${util.format('%%')}"`);
console.log(`  format('Hi', 'there'):        "${util.format('Hi', 'there')}"`);

// 2. util.inspect
console.log(`\n${c.bold}2. util.inspect${c.reset}`);

const complexObj = {
  name: 'Test',
  nested: {
    level1: {
      level2: {
        level3: 'deep'
      }
    }
  },
  array: [1, 2, 3],
  fn: function example() {},
  sym: Symbol('test'),
  date: new Date(),
  regex: /test/gi,
  map: new Map([['a', 1]]),
  set: new Set([1, 2, 3]),
};

console.log(`  Default (depth 2):`);
console.log(`  ${c.dim}${util.inspect(complexObj, { depth: 2 }).split('\n').join('\n  ')}${c.reset}`);

console.log(`\n  With colors (showHidden):`);
console.log(`  ${util.inspect(complexObj, { colors: true, depth: 1, showHidden: true }).split('\n').join('\n  ')}`);

// 3. util.promisify
console.log(`\n${c.bold}3. util.promisify${c.reset}`);

// Callback-style function
function oldStyleAsync(value, callback) {
  setTimeout(() => {
    if (value < 0) {
      callback(new Error('Value must be positive'));
    } else {
      callback(null, value * 2);
    }
  }, 10);
}

const promisified = util.promisify(oldStyleAsync);

try {
  const result = await promisified(21);
  console.log(`  promisified(21): ${result}`);
} catch (e) {
  console.log(`  Error: ${e.message}`);
}

// With fs
const readFileAsync = util.promisify(fs.readFile);
const ROOT = path.dirname(process.argv[1] || __filename);
const testFile = path.join(ROOT, 'test.txt');

fs.writeFileSync(testFile, 'Hello from promisify!');
const content = await readFileAsync(testFile, 'utf8');
console.log(`  Async file read: "${content}"`);
fs.unlinkSync(testFile);

// 4. util.callbackify
console.log(`\n${c.bold}4. util.callbackify${c.reset}`);

async function asyncDouble(value) {
  return value * 2;
}

const callbackified = util.callbackify(asyncDouble);

await new Promise((resolve) => {
  callbackified(10, (err, result) => {
    console.log(`  callbackified(10): ${result}`);
    resolve();
  });
});

// 5. util.types
console.log(`\n${c.bold}5. util.types${c.reset}`);

if (util.types) {
  const checks = [
    ['isDate', new Date()],
    ['isRegExp', /test/],
    ['isMap', new Map()],
    ['isSet', new Set()],
    ['isPromise', Promise.resolve()],
    ['isArrayBuffer', new ArrayBuffer(8)],
    ['isTypedArray', new Uint8Array(8)],
    ['isGeneratorFunction', function* () {}],
    ['isAsyncFunction', async function () {}],
  ];

  for (const [method, value] of checks) {
    if (util.types[method]) {
      const result = util.types[method](value);
      const typeName = value.constructor.name;
      console.log(`  ${method}(${typeName}): ${result}`);
    }
  }
} else {
  console.log(`  ${c.dim}(util.types not available)${c.reset}`);
}

// 6. util.deprecate
console.log(`\n${c.bold}6. util.deprecate${c.reset}`);

const oldFunction = util.deprecate(() => {
  return 'old result';
}, 'oldFunction is deprecated, use newFunction instead');

// Note: Warning only shows once per unique message
const result = oldFunction();
console.log(`  Deprecated function returned: "${result}"`);
console.log(`  ${c.dim}(Deprecation warning may appear in stderr)${c.reset}`);

// 7. util.inherits (legacy)
console.log(`\n${c.bold}7. util.inherits (legacy)${c.reset}`);

function Animal(name) {
  this.name = name;
}

Animal.prototype.speak = function() {
  return `${this.name} makes a sound`;
};

function Dog(name) {
  Animal.call(this, name);
}

util.inherits(Dog, Animal);

Dog.prototype.speak = function() {
  return `${this.name} barks!`;
};

const dog = new Dog('Rex');
console.log(`  Dog speaks: "${dog.speak()}"`);
console.log(`  Dog instanceof Animal: ${dog instanceof Animal}`);
console.log(`  ${c.dim}(Note: Use ES6 classes instead)${c.reset}`);

// 8. util.isDeepStrictEqual
console.log(`\n${c.bold}8. util.isDeepStrictEqual${c.reset}`);

if (util.isDeepStrictEqual) {
  const obj1 = { a: 1, b: { c: 2 } };
  const obj2 = { a: 1, b: { c: 2 } };
  const obj3 = { a: 1, b: { c: 3 } };

  console.log(`  obj1 === obj2: ${obj1 === obj2}`);
  console.log(`  isDeepStrictEqual(obj1, obj2): ${util.isDeepStrictEqual(obj1, obj2)}`);
  console.log(`  isDeepStrictEqual(obj1, obj3): ${util.isDeepStrictEqual(obj1, obj3)}`);

  // With arrays
  const arr1 = [1, [2, 3]];
  const arr2 = [1, [2, 3]];
  console.log(`  isDeepStrictEqual(arr1, arr2): ${util.isDeepStrictEqual(arr1, arr2)}`);
} else {
  console.log(`  ${c.dim}(isDeepStrictEqual not available)${c.reset}`);
}

// 9. util.debuglog
console.log(`\n${c.bold}9. util.debuglog${c.reset}`);

const debuglog = util.debuglog('myapp');
debuglog('This is a debug message (set NODE_DEBUG=myapp to see)');
console.log(`  Debug logger created for 'myapp'`);
console.log(`  ${c.dim}Set NODE_DEBUG=myapp to enable${c.reset}`);

// 10. Text encoding/decoding
console.log(`\n${c.bold}10. TextEncoder/TextDecoder${c.reset}`);

if (typeof TextEncoder !== 'undefined') {
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  const text = 'Hello, 世界!';
  const encoded = encoder.encode(text);
  const decoded = decoder.decode(encoded);

  console.log(`  Original: "${text}"`);
  console.log(`  Encoded: Uint8Array(${encoded.length}) [${[...encoded.slice(0, 10)].join(', ')}...]`);
  console.log(`  Decoded: "${decoded}"`);
} else {
  // Fallback using Buffer
  const text = 'Hello, 世界!';
  const encoded = Buffer.from(text, 'utf8');
  const decoded = encoded.toString('utf8');

  console.log(`  Original: "${text}"`);
  console.log(`  Encoded (Buffer): ${encoded.length} bytes`);
  console.log(`  Decoded: "${decoded}"`);
}

// 11. Custom inspect
console.log(`\n${c.bold}11. Custom Inspect${c.reset}`);

class Person {
  constructor(name, age) {
    this.name = name;
    this.age = age;
    this.secret = 'hidden';
  }

  [util.inspect.custom](depth, options) {
    return `Person { name: "${this.name}", age: ${this.age} }`;
  }
}

const person = new Person('Alice', 30);
console.log(`  Default inspect: ${util.inspect(person)}`);

// 12. Formatting helpers
console.log(`\n${c.bold}12. Formatting Helpers${c.reset}`);

function formatTable(data) {
  if (data.length === 0) return '(empty)';

  const keys = Object.keys(data[0]);
  const widths = keys.map(k =>
    Math.max(k.length, ...data.map(row => String(row[k]).length))
  );

  const header = keys.map((k, i) => k.padEnd(widths[i])).join(' | ');
  const separator = widths.map(w => '-'.repeat(w)).join('-+-');
  const rows = data.map(row =>
    keys.map((k, i) => String(row[k]).padEnd(widths[i])).join(' | ')
  );

  return [header, separator, ...rows].join('\n');
}

const tableData = [
  { name: 'Alice', age: 30, city: 'NYC' },
  { name: 'Bob', age: 25, city: 'LA' },
  { name: 'Charlie', age: 35, city: 'Chicago' },
];

console.log(`  Table format:`);
console.log(`  ${formatTable(tableData).split('\n').join('\n  ')}`);

console.log(`\n${c.green}${c.bold}Util helpers demo completed!${c.reset}\n`);
