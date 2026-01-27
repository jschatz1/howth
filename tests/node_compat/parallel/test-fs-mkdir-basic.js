'use strict';
const assert = require('assert');
const fs = require('fs');
const path = require('path');

const tmpdir = require('../common/tmpdir');
tmpdir.refresh();

const testDir = path.join(tmpdir.path, 'test-mkdir');
const nestedDir = path.join(tmpdir.path, 'test-mkdir-nested', 'a', 'b', 'c');

// Test mkdirSync
fs.mkdirSync(testDir);
assert.strictEqual(fs.existsSync(testDir), true);
const stat = fs.statSync(testDir);
assert.strictEqual(stat.isDirectory(), true);

// Test mkdirSync with recursive
fs.mkdirSync(nestedDir, { recursive: true });
assert.strictEqual(fs.existsSync(nestedDir), true);

// Test async mkdir
const asyncDir = path.join(tmpdir.path, 'test-mkdir-async');
fs.mkdir(asyncDir, (err) => {
  assert.strictEqual(err, null);
  assert.strictEqual(fs.existsSync(asyncDir), true);

  // Test promises API
  const promiseDir = path.join(tmpdir.path, 'test-mkdir-promise');
  fs.promises.mkdir(promiseDir).then(() => {
    assert.strictEqual(fs.existsSync(promiseDir), true);
    console.log('All mkdir tests passed!');
  });
});
