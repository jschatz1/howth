'use strict';
// Basic URL tests that don't require node:test or vm
const assert = require('assert');

// URL constructor
const url1 = new URL('https://example.com:8080/path?query=value#hash');
assert.strictEqual(url1.protocol, 'https:');
assert.strictEqual(url1.hostname, 'example.com');
assert.strictEqual(url1.port, '8080');
assert.strictEqual(url1.pathname, '/path');
assert.strictEqual(url1.search, '?query=value');
assert.strictEqual(url1.hash, '#hash');
assert.strictEqual(url1.host, 'example.com:8080');
assert.strictEqual(url1.origin, 'https://example.com:8080');

// URL without port
const url2 = new URL('http://example.org/foo/bar');
assert.strictEqual(url2.port, '');
assert.strictEqual(url2.host, 'example.org');

// URL toString
assert.strictEqual(url1.toString(), 'https://example.com:8080/path?query=value#hash');

// Relative URL with base
const url5 = new URL('/path', 'https://example.com');
assert.strictEqual(url5.href, 'https://example.com/path');

// URLSearchParams
const params1 = new URLSearchParams('foo=bar&baz=qux');
assert.strictEqual(params1.get('foo'), 'bar');
assert.strictEqual(params1.get('baz'), 'qux');
assert.strictEqual(params1.get('missing'), null);

// URLSearchParams set
params1.set('foo', 'newvalue');
assert.strictEqual(params1.get('foo'), 'newvalue');

// URLSearchParams append
params1.append('foo', 'another');
assert.strictEqual(params1.getAll('foo').length, 2);

// URLSearchParams has
assert.strictEqual(params1.has('foo'), true);
assert.strictEqual(params1.has('missing'), false);

// URLSearchParams delete
params1.delete('foo');
assert.strictEqual(params1.has('foo'), false);

// URLSearchParams toString
const params2 = new URLSearchParams();
params2.set('a', '1');
params2.set('b', '2');
assert.strictEqual(params2.toString(), 'a=1&b=2');

// URLSearchParams from URL
const url6 = new URL('https://example.com?x=1&y=2');
assert.strictEqual(url6.searchParams.get('x'), '1');
assert.strictEqual(url6.searchParams.get('y'), '2');

// URLSearchParams iterator
const params3 = new URLSearchParams('a=1&b=2&c=3');
const entries = [];
for (const [key, value] of params3) {
  entries.push([key, value]);
}
assert.strictEqual(entries.length, 3);
assert.deepStrictEqual(entries[0], ['a', '1']);

// URLSearchParams keys/values
const keys = [...params3.keys()];
const values = [...params3.values()];
assert.deepStrictEqual(keys, ['a', 'b', 'c']);
assert.deepStrictEqual(values, ['1', '2', '3']);

// URL encoding in search params
const params4 = new URLSearchParams();
params4.set('name', 'hello world');
assert.strictEqual(params4.toString(), 'name=hello%20world');

console.log('All URL tests passed!');
