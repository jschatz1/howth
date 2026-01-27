'use strict';
const assert = require('assert');
const querystring = require('querystring');

// Test parse
const parsed = querystring.parse('foo=bar&baz=qux');
assert.strictEqual(parsed.foo, 'bar');
assert.strictEqual(parsed.baz, 'qux');

// Test parse with URL encoding
const parsedEncoded = querystring.parse('name=John%20Doe&city=New%20York');
assert.strictEqual(parsedEncoded.name, 'John Doe');
assert.strictEqual(parsedEncoded.city, 'New York');

// Test parse with plus sign as space
const parsedPlus = querystring.parse('name=John+Doe');
assert.strictEqual(parsedPlus.name, 'John Doe');

// Test parse with multiple values for same key
const parsedMulti = querystring.parse('foo=1&foo=2&foo=3');
assert.deepStrictEqual(parsedMulti.foo, ['1', '2', '3']);

// Test parse with empty value
const parsedEmpty = querystring.parse('foo=&bar');
assert.strictEqual(parsedEmpty.foo, '');
assert.strictEqual(parsedEmpty.bar, '');

// Test parse with custom separator
const parsedSep = querystring.parse('foo:bar;baz:qux', ';', ':');
assert.strictEqual(parsedSep.foo, 'bar');
assert.strictEqual(parsedSep.baz, 'qux');

// Test stringify
const stringified = querystring.stringify({ foo: 'bar', baz: 'qux' });
assert.ok(stringified === 'foo=bar&baz=qux' || stringified === 'baz=qux&foo=bar');

// Test stringify with array
const stringifiedArr = querystring.stringify({ foo: ['1', '2', '3'] });
assert.strictEqual(stringifiedArr, 'foo=1&foo=2&foo=3');

// Test stringify with URL encoding
const stringifiedEnc = querystring.stringify({ name: 'John Doe' });
assert.strictEqual(stringifiedEnc, 'name=John%20Doe');

// Test stringify with custom separator
const stringifiedSep = querystring.stringify({ foo: 'bar', baz: 'qux' }, ';', ':');
assert.ok(stringifiedSep === 'foo:bar;baz:qux' || stringifiedSep === 'baz:qux;foo:bar');

// Test escape
const escaped = querystring.escape('hello world');
assert.strictEqual(escaped, 'hello%20world');

// Test unescape
const unescaped = querystring.unescape('hello%20world');
assert.strictEqual(unescaped, 'hello world');

// Test unescape with plus
const unescapedPlus = querystring.unescape('hello+world');
assert.strictEqual(unescapedPlus, 'hello world');

// Test decode alias
assert.strictEqual(querystring.decode, querystring.parse);

// Test encode alias
assert.strictEqual(querystring.encode, querystring.stringify);

console.log('All querystring module tests passed!');
