// Test readline module
const readline = require('readline');
const { Readable, Writable } = require('stream');

console.log('Testing readline module...\n');

let passed = 0;
let failed = 0;

function test(name, fn) {
  try {
    fn();
    console.log(`✓ ${name}`);
    passed++;
  } catch (e) {
    console.log(`✗ ${name}: ${e.message}`);
    failed++;
  }
}

async function asyncTest(name, fn) {
  try {
    await fn();
    console.log(`✓ ${name}`);
    passed++;
  } catch (e) {
    console.log(`✗ ${name}: ${e.message}`);
    failed++;
  }
}

// Test basic module existence
test('readline module exists', () => {
  if (typeof readline !== 'object') throw new Error('readline is not an object');
});

test('readline.createInterface exists', () => {
  if (typeof readline.createInterface !== 'function') {
    throw new Error('createInterface is not a function');
  }
});

test('readline.promises exists', () => {
  if (!readline.promises) throw new Error('promises not found');
  if (typeof readline.promises.createInterface !== 'function') {
    throw new Error('promises.createInterface is not a function');
  }
});

// Test cursor functions
test('readline.cursorTo exists', () => {
  if (typeof readline.cursorTo !== 'function') {
    throw new Error('cursorTo is not a function');
  }
});

test('readline.moveCursor exists', () => {
  if (typeof readline.moveCursor !== 'function') {
    throw new Error('moveCursor is not a function');
  }
});

test('readline.clearLine exists', () => {
  if (typeof readline.clearLine !== 'function') {
    throw new Error('clearLine is not a function');
  }
});

test('readline.clearScreenDown exists', () => {
  if (typeof readline.clearScreenDown !== 'function') {
    throw new Error('clearScreenDown is not a function');
  }
});

test('readline.emitKeypressEvents exists', () => {
  if (typeof readline.emitKeypressEvents !== 'function') {
    throw new Error('emitKeypressEvents is not a function');
  }
});

// Test cursor functions write correct ANSI codes
test('cursorTo writes correct ANSI code for column', () => {
  let output = '';
  const mockStream = { write: (data) => { output += data; } };
  readline.cursorTo(mockStream, 5);
  if (output !== '\x1b[6G') {
    throw new Error(`Expected '\\x1b[6G' but got '${output.replace(/\x1b/g, '\\x1b')}'`);
  }
});

test('cursorTo writes correct ANSI code for position', () => {
  let output = '';
  const mockStream = { write: (data) => { output += data; } };
  readline.cursorTo(mockStream, 10, 5);
  if (output !== '\x1b[6;11H') {
    throw new Error(`Expected '\\x1b[6;11H' but got '${output.replace(/\x1b/g, '\\x1b')}'`);
  }
});

test('moveCursor writes correct ANSI codes', () => {
  let output = '';
  const mockStream = { write: (data) => { output += data; } };
  readline.moveCursor(mockStream, 3, -2);
  if (output !== '\x1b[3C\x1b[2A') {
    throw new Error(`Expected '\\x1b[3C\\x1b[2A' but got '${output.replace(/\x1b/g, '\\x1b')}'`);
  }
});

test('clearLine writes correct ANSI code', () => {
  let output = '';
  const mockStream = { write: (data) => { output += data; } };
  readline.clearLine(mockStream, 0);
  if (output !== '\x1b[2K') {
    throw new Error(`Expected '\\x1b[2K' but got '${output.replace(/\x1b/g, '\\x1b')}'`);
  }
});

test('clearScreenDown writes correct ANSI code', () => {
  let output = '';
  const mockStream = { write: (data) => { output += data; } };
  readline.clearScreenDown(mockStream);
  if (output !== '\x1b[0J') {
    throw new Error(`Expected '\\x1b[0J' but got '${output.replace(/\x1b/g, '\\x1b')}'`);
  }
});

// Test createInterface
test('createInterface returns object with expected methods', () => {
  const input = new Readable({ read() {} });
  const rl = readline.createInterface({ input });

  if (typeof rl.question !== 'function') throw new Error('missing question');
  if (typeof rl.close !== 'function') throw new Error('missing close');
  if (typeof rl.prompt !== 'function') throw new Error('missing prompt');
  if (typeof rl.write !== 'function') throw new Error('missing write');
  if (typeof rl.pause !== 'function') throw new Error('missing pause');
  if (typeof rl.resume !== 'function') throw new Error('missing resume');
  if (typeof rl.setPrompt !== 'function') throw new Error('missing setPrompt');
  if (typeof rl[Symbol.asyncIterator] !== 'function') throw new Error('missing asyncIterator');

  rl.close();
});

test('createInterface is an EventEmitter', () => {
  const input = new Readable({ read() {} });
  const rl = readline.createInterface({ input });

  if (typeof rl.on !== 'function') throw new Error('missing on');
  if (typeof rl.emit !== 'function') throw new Error('missing emit');
  if (typeof rl.once !== 'function') throw new Error('missing once');

  rl.close();
});

// Test line parsing from stream
async function runAsyncTests() {
  await asyncTest('createInterface emits line events from input stream', async () => {
    const input = new Readable({ read() {} });
    const rl = readline.createInterface({ input });

    const lines = [];
    rl.on('line', (line) => lines.push(line));

    // Push some data
    input.push('hello\n');
    input.push('world\n');
    input.push(null); // EOF

    // Wait for processing
    await new Promise(resolve => setTimeout(resolve, 50));

    if (lines.length !== 2) throw new Error(`Expected 2 lines, got ${lines.length}`);
    if (lines[0] !== 'hello') throw new Error(`Expected 'hello', got '${lines[0]}'`);
    if (lines[1] !== 'world') throw new Error(`Expected 'world', got '${lines[1]}'`);

    rl.close();
  });

  await asyncTest('createInterface handles CRLF line endings', async () => {
    const input = new Readable({ read() {} });
    const rl = readline.createInterface({ input });

    const lines = [];
    rl.on('line', (line) => lines.push(line));

    input.push('line1\r\n');
    input.push('line2\r\n');
    input.push(null);

    await new Promise(resolve => setTimeout(resolve, 50));

    if (lines.length !== 2) throw new Error(`Expected 2 lines, got ${lines.length}`);
    if (lines[0] !== 'line1') throw new Error(`Line 1 has trailing \\r`);
    if (lines[1] !== 'line2') throw new Error(`Line 2 has trailing \\r`);

    rl.close();
  });

  await asyncTest('createInterface async iterator works', async () => {
    const input = new Readable({ read() {} });
    const rl = readline.createInterface({ input });

    // Push data before iterating
    input.push('first\n');
    input.push('second\n');
    input.push('third\n');
    input.push(null);

    // Wait for buffering
    await new Promise(resolve => setTimeout(resolve, 50));

    const lines = [];
    for await (const line of rl) {
      lines.push(line);
    }

    if (lines.length !== 3) throw new Error(`Expected 3 lines, got ${lines.length}`);
    if (lines[0] !== 'first') throw new Error('Wrong first line');
    if (lines[1] !== 'second') throw new Error('Wrong second line');
    if (lines[2] !== 'third') throw new Error('Wrong third line');
  });

  await asyncTest('close event is emitted', async () => {
    const input = new Readable({ read() {} });
    const rl = readline.createInterface({ input });

    let closed = false;
    rl.on('close', () => { closed = true; });

    rl.close();

    await new Promise(resolve => setTimeout(resolve, 10));

    if (!closed) throw new Error('close event not emitted');
  });

  await asyncTest('question with stream input calls callback', async () => {
    const input = new Readable({ read() {} });
    let outputData = '';
    const output = new Writable({
      write(chunk, encoding, callback) {
        outputData += chunk.toString();
        callback();
      }
    });

    const rl = readline.createInterface({ input, output });

    // Start question
    const answerPromise = new Promise(resolve => {
      rl.question('What is your name? ', (answer) => {
        resolve(answer);
      });
    });

    // Give time for the question to be written
    await new Promise(resolve => setTimeout(resolve, 10));

    // Push answer
    input.push('Claude\n');

    const answer = await Promise.race([
      answerPromise,
      new Promise((_, reject) => setTimeout(() => reject(new Error('Timeout')), 500))
    ]);

    if (outputData !== 'What is your name? ') {
      throw new Error(`Prompt not written, got: '${outputData}'`);
    }
    if (answer !== 'Claude') {
      throw new Error(`Expected 'Claude', got '${answer}'`);
    }

    rl.close();
  });

  await asyncTest('readline/promises question returns promise', async () => {
    const readlinePromises = require('readline/promises');

    const input = new Readable({ read() {} });
    const output = new Writable({ write(c, e, cb) { cb(); } });
    const rl = readlinePromises.createInterface({ input, output });

    // Start question (returns promise)
    const questionPromise = rl.question('Enter: ');

    // Verify it's a promise
    if (!(questionPromise instanceof Promise)) {
      throw new Error('question() did not return a Promise');
    }

    // Push answer
    await new Promise(resolve => setTimeout(resolve, 10));
    input.push('test\n');

    const answer = await Promise.race([
      questionPromise,
      new Promise((_, reject) => setTimeout(() => reject(new Error('Timeout')), 500))
    ]);

    if (answer !== 'test') {
      throw new Error(`Expected 'test', got '${answer}'`);
    }

    rl.close();
  });

  await asyncTest('setPrompt and prompt work together', async () => {
    const input = new Readable({ read() {} });
    let outputData = '';
    const output = new Writable({
      write(chunk, encoding, callback) {
        outputData += chunk.toString();
        callback();
      }
    });

    const rl = readline.createInterface({ input, output });

    rl.setPrompt('>>> ');
    rl.prompt();

    if (outputData !== '>>> ') {
      throw new Error(`Expected '>>> ', got '${outputData}'`);
    }

    rl.close();
  });

  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
  process.exit(failed > 0 ? 1 : 0);
}

runAsyncTests().catch(e => {
  console.error('Test error:', e);
  process.exit(1);
});
