'use strict';
const assert = require('assert');
const stream = require('stream');
const { Readable, Writable, Duplex, Transform, PassThrough, pipeline, finished } = stream;

// Test Stream module exports
assert.strictEqual(typeof Readable, 'function');
assert.strictEqual(typeof Writable, 'function');
assert.strictEqual(typeof Duplex, 'function');
assert.strictEqual(typeof Transform, 'function');
assert.strictEqual(typeof PassThrough, 'function');
assert.strictEqual(typeof pipeline, 'function');
assert.strictEqual(typeof finished, 'function');

// Test Readable stream
const readableData = ['chunk1', 'chunk2', 'chunk3'];
let readIndex = 0;
const readable = new Readable({
  read() {
    if (readIndex < readableData.length) {
      this.push(readableData[readIndex++]);
    } else {
      this.push(null);
    }
  }
});

const chunks = [];
readable.on('data', (chunk) => {
  chunks.push(chunk.toString());
});
readable.on('end', () => {
  assert.deepStrictEqual(chunks, ['chunk1', 'chunk2', 'chunk3']);
});

// Test Writable stream
const writtenData = [];
const writable = new Writable({
  write(chunk, encoding, callback) {
    writtenData.push(chunk.toString());
    callback();
  }
});

writable.write('hello');
writable.write('world');
writable.end(() => {
  assert.deepStrictEqual(writtenData, ['hello', 'world']);
});

// Test Transform stream
const transform = new Transform({
  transform(chunk, encoding, callback) {
    callback(null, chunk.toString().toUpperCase());
  }
});

const transformedChunks = [];
transform.on('data', (chunk) => {
  transformedChunks.push(chunk.toString());
});
transform.write('hello');
transform.write('world');
transform.end(() => {
  assert.deepStrictEqual(transformedChunks, ['HELLO', 'WORLD']);
});

// Test PassThrough stream
const passThrough = new PassThrough();
const passThroughChunks = [];
passThrough.on('data', (chunk) => {
  passThroughChunks.push(chunk.toString());
});
passThrough.write('pass');
passThrough.write('through');
passThrough.end(() => {
  assert.deepStrictEqual(passThroughChunks, ['pass', 'through']);
});

// Test pipe
const pipeSource = new Readable({
  read() {
    this.push('piped data');
    this.push(null);
  }
});
const pipeDest = new Writable({
  write(chunk, encoding, callback) {
    assert.strictEqual(chunk.toString(), 'piped data');
    callback();
  }
});
pipeSource.pipe(pipeDest);

// Test Duplex stream
const duplex = new Duplex({
  read() {
    this.push('duplex read');
    this.push(null);
  },
  write(chunk, encoding, callback) {
    callback();
  }
});
assert.strictEqual(duplex.readable, true);
assert.strictEqual(duplex.writable, true);

// Test stream events
const eventStream = new Readable({
  read() {
    this.push('data');
    this.push(null);
  }
});
let dataEmitted = false;
let endEmitted = false;
eventStream.on('data', () => { dataEmitted = true; });
eventStream.on('end', () => { endEmitted = true; });
eventStream.resume();

// Test Readable.from
const fromIterable = Readable.from(['a', 'b', 'c']);
const fromChunks = [];
fromIterable.on('data', (chunk) => fromChunks.push(chunk.toString()));
fromIterable.on('end', () => {
  assert.deepStrictEqual(fromChunks, ['a', 'b', 'c']);
});

// Test stream promises
assert.strictEqual(typeof stream.promises, 'object');
assert.strictEqual(typeof stream.promises.pipeline, 'function');
assert.strictEqual(typeof stream.promises.finished, 'function');

// Test finished function
const finishedStream = new Readable({
  read() {
    this.push('data');
    this.push(null);
  }
});
let finishedCalled = false;
finished(finishedStream, () => {
  finishedCalled = true;
});
finishedStream.resume();

// Test pause/resume
const pauseStream = new Readable({
  read() {
    this.push('pause test');
    this.push(null);
  }
});
pauseStream.pause();
assert.strictEqual(pauseStream.isPaused(), true);
pauseStream.resume();
assert.strictEqual(pauseStream.isPaused(), false);

// Test setEncoding
const encodingStream = new Readable({
  read() {
    this.push(Buffer.from('encoded'));
    this.push(null);
  }
});
encodingStream.setEncoding('utf8');

// Test cork/uncork
const corkStream = new Writable({
  write(chunk, encoding, callback) {
    callback();
  }
});
corkStream.cork();
corkStream.write('corked');
corkStream.uncork();

// Test destroy
const destroyStream = new Readable({ read() {} });
destroyStream.destroy();

// Allow async operations to complete
setTimeout(() => {
  assert.strictEqual(dataEmitted, true);
  assert.strictEqual(endEmitted, true);
  console.log('All stream tests passed!');
}, 50);
