'use strict';
const assert = require('assert');
const fs = require('fs');
const path = require('path');

const tmpdir = require('../common/tmpdir');
tmpdir.refresh();

const testFile = path.join(tmpdir.path, 'test-writefile.txt');

// Test writeFileSync with string
fs.writeFileSync(testFile, 'hello world');
assert.strictEqual(fs.readFileSync(testFile, 'utf8'), 'hello world');

// Test writeFileSync with Buffer
fs.writeFileSync(testFile, Buffer.from('buffer data'));
assert.strictEqual(fs.readFileSync(testFile, 'utf8'), 'buffer data');

// Test writeFileSync overwrites
fs.writeFileSync(testFile, 'first');
fs.writeFileSync(testFile, 'second');
assert.strictEqual(fs.readFileSync(testFile, 'utf8'), 'second');

// Test async writeFile
fs.writeFile(testFile, 'async content', (err) => {
  assert.strictEqual(err, null);
  assert.strictEqual(fs.readFileSync(testFile, 'utf8'), 'async content');

  // Test promises API
  fs.promises.writeFile(testFile, 'promises content').then(() => {
    assert.strictEqual(fs.readFileSync(testFile, 'utf8'), 'promises content');
    console.log('All writeFile tests passed!');
  });
});
