'use strict';
const assert = require('assert');
const util = require('util');

// Test util.format
assert.strictEqual(util.format('hello'), 'hello');
assert.strictEqual(util.format('hello %s', 'world'), 'hello world');
assert.strictEqual(util.format('%d + %d = %d', 1, 2, 3), '1 + 2 = 3');
assert.strictEqual(util.format('%i', 42.5), '42');
assert.strictEqual(util.format('%f', 3.14), '3.14');
assert.strictEqual(util.format('%%'), '%');
assert.strictEqual(util.format('%j', { a: 1 }), '{"a":1}');
assert.strictEqual(util.format('a', 'b', 'c'), 'a "b" "c"');

// Test util.inspect
assert.strictEqual(util.inspect(null), 'null');
assert.strictEqual(util.inspect(undefined), 'undefined');
assert.strictEqual(util.inspect(42), '42');
assert.strictEqual(util.inspect(true), 'true');
assert.strictEqual(util.inspect('hello'), '"hello"');
assert.strictEqual(util.inspect([1, 2, 3]), '[ 1, 2, 3 ]');
assert.strictEqual(util.inspect({ a: 1 }), '{ a: 1 }');
assert.strictEqual(util.inspect(new Date('2024-01-01')), '2024-01-01T00:00:00.000Z');
assert.strictEqual(util.inspect(/abc/g), '/abc/g');
assert.ok(util.inspect(new Map([[1, 2]])).includes('Map'));
assert.ok(util.inspect(new Set([1, 2])).includes('Set'));
assert.ok(util.inspect(function foo() {}).includes('Function'));

// Test circular reference detection
const circular = { a: 1 };
circular.self = circular;
assert.ok(util.inspect(circular).includes('[Circular]'));

// Test util.inspect.custom
const obj = {
  [util.inspect.custom]() {
    return 'custom inspect';
  }
};
// Note: custom inspect requires special handling

// Test util.promisify
function callbackFn(a, b, callback) {
  setTimeout(() => callback(null, a + b), 0);
}
const promisified = util.promisify(callbackFn);
assert.strictEqual(typeof promisified, 'function');

// Test util.callbackify
async function asyncFn(a, b) {
  return a + b;
}
const callbackified = util.callbackify(asyncFn);
assert.strictEqual(typeof callbackified, 'function');

// Test util.deprecate
const deprecated = util.deprecate(() => 'result', 'This is deprecated');
assert.strictEqual(deprecated(), 'result');

// Test util.inherits
function Parent() { this.name = 'parent'; }
Parent.prototype.greet = function() { return 'hello'; };
function Child() { Parent.call(this); }
util.inherits(Child, Parent);
const child = new Child();
assert.strictEqual(child.greet(), 'hello');
assert.strictEqual(Child.super_, Parent);

// Test util.types
assert.strictEqual(util.types.isDate(new Date()), true);
assert.strictEqual(util.types.isDate({}), false);
assert.strictEqual(util.types.isRegExp(/abc/), true);
assert.strictEqual(util.types.isRegExp('abc'), false);
assert.strictEqual(util.types.isMap(new Map()), true);
assert.strictEqual(util.types.isSet(new Set()), true);
assert.strictEqual(util.types.isPromise(Promise.resolve()), true);
assert.strictEqual(util.types.isArrayBuffer(new ArrayBuffer(8)), true);
assert.strictEqual(util.types.isTypedArray(new Uint8Array(8)), true);
assert.strictEqual(util.types.isTypedArray(new ArrayBuffer(8)), false);
assert.strictEqual(util.types.isUint8Array(new Uint8Array(8)), true);
assert.strictEqual(util.types.isDataView(new DataView(new ArrayBuffer(8))), true);
assert.strictEqual(util.types.isNativeError(new Error()), true);
assert.strictEqual(util.types.isNativeError(new TypeError()), true);

// Test util.isDeepStrictEqual
assert.strictEqual(util.isDeepStrictEqual({ a: 1 }, { a: 1 }), true);
assert.strictEqual(util.isDeepStrictEqual({ a: 1 }, { a: 2 }), false);
assert.strictEqual(util.isDeepStrictEqual([1, 2, 3], [1, 2, 3]), true);
assert.strictEqual(util.isDeepStrictEqual([1, 2], [1, 2, 3]), false);
assert.strictEqual(util.isDeepStrictEqual(new Map([[1, 2]]), new Map([[1, 2]])), true);
assert.strictEqual(util.isDeepStrictEqual(new Set([1, 2]), new Set([1, 2])), true);

// Test legacy type checking
assert.strictEqual(util.isArray([]), true);
assert.strictEqual(util.isBoolean(true), true);
assert.strictEqual(util.isNull(null), true);
assert.strictEqual(util.isNullOrUndefined(null), true);
assert.strictEqual(util.isNullOrUndefined(undefined), true);
assert.strictEqual(util.isNumber(42), true);
assert.strictEqual(util.isString('hello'), true);
assert.strictEqual(util.isSymbol(Symbol()), true);
assert.strictEqual(util.isUndefined(undefined), true);
assert.strictEqual(util.isRegExp(/abc/), true);
assert.strictEqual(util.isObject({}), true);
assert.strictEqual(util.isObject(null), false);
assert.strictEqual(util.isDate(new Date()), true);
assert.strictEqual(util.isError(new Error()), true);
assert.strictEqual(util.isFunction(() => {}), true);
assert.strictEqual(util.isPrimitive(42), true);
assert.strictEqual(util.isPrimitive({}), false);
assert.strictEqual(util.isBuffer(Buffer.from('test')), true);

// Test util.debuglog
const debug = util.debuglog('test');
assert.strictEqual(typeof debug, 'function');
assert.strictEqual(typeof debug.enabled, 'boolean');

// Test promisify.custom
assert.strictEqual(typeof util.promisify.custom, 'symbol');

// Test inspect.custom
assert.strictEqual(typeof util.inspect.custom, 'symbol');

// Test TextEncoder/TextDecoder exports
assert.strictEqual(util.TextEncoder, TextEncoder);
assert.strictEqual(util.TextDecoder, TextDecoder);

console.log('All util tests passed!');
