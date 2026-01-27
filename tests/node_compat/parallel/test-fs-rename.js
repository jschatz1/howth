'use strict';
const assert = require('assert');
const fs = require('fs');
const path = require('path');

const tmpdir = require('../common/tmpdir');
tmpdir.refresh();

const testFile = path.join(tmpdir.path, 'test-rename-src.txt');
const destFile = path.join(tmpdir.path, 'test-rename-dest.txt');

// Test renameSync
fs.writeFileSync(testFile, 'rename content');
fs.renameSync(testFile, destFile);
assert.strictEqual(fs.existsSync(testFile), false);
assert.strictEqual(fs.existsSync(destFile), true);
assert.strictEqual(fs.readFileSync(destFile, 'utf8'), 'rename content');

// Clean up for next test
fs.unlinkSync(destFile);

// Test async rename
fs.writeFileSync(testFile, 'async rename content');
fs.rename(testFile, destFile, (err) => {
  assert.strictEqual(err, null);
  assert.strictEqual(fs.existsSync(testFile), false);
  assert.strictEqual(fs.existsSync(destFile), true);
  assert.strictEqual(fs.readFileSync(destFile, 'utf8'), 'async rename content');

  // Clean up for next test
  fs.unlinkSync(destFile);

  // Test promises API
  fs.writeFileSync(testFile, 'promises rename content');
  fs.promises.rename(testFile, destFile).then(() => {
    assert.strictEqual(fs.existsSync(testFile), false);
    assert.strictEqual(fs.existsSync(destFile), true);
    assert.strictEqual(fs.readFileSync(destFile, 'utf8'), 'promises rename content');
    console.log('All rename tests passed!');
  });
});
