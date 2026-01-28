/**
 * Stream Processing Example
 *
 * Demonstrates Node.js streams:
 * - Readable streams
 * - Writable streams
 * - Transform streams
 * - Piping and chaining
 * - Stream events
 *
 * Run: howth run --native examples/stream-pipeline/streams.js
 */

const { Readable, Writable, Transform, PassThrough } = require('stream');

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
};

console.log(`\n${c.bold}${c.cyan}Stream Processing Demo${c.reset}\n`);

// 1. Custom Readable Stream
console.log(`${c.bold}1. Custom Readable Stream${c.reset}`);

class NumberStream extends Readable {
  constructor(max = 5) {
    super({ objectMode: true });
    this.current = 1;
    this.max = max;
  }

  _read() {
    if (this.current <= this.max) {
      this.push({ number: this.current, squared: this.current * this.current });
      this.current++;
    } else {
      this.push(null);
    }
  }
}

const numberStream = new NumberStream(5);
const numbers = [];
numberStream.on('data', chunk => numbers.push(chunk));
numberStream.on('end', () => {
  console.log(`  Generated: ${JSON.stringify(numbers)}`);
});

// Wait for stream to complete
setTimeout(() => {
  // 2. Custom Writable Stream
  console.log(`\n${c.bold}2. Custom Writable Stream${c.reset}`);

  class LogWriter extends Writable {
    constructor() {
      super({ objectMode: true });
      this.logs = [];
    }

    _write(chunk, encoding, callback) {
      this.logs.push(`[${new Date().toISOString()}] ${chunk}`);
      callback();
    }
  }

  const logger = new LogWriter();
  logger.write('Application started');
  logger.write('Processing data...');
  logger.write('Task completed');
  logger.end();

  console.log(`  Logs collected: ${logger.logs.length}`);
  logger.logs.forEach(log => console.log(`  ${c.dim}${log}${c.reset}`));

  // 3. Transform Stream
  console.log(`\n${c.bold}3. Transform Stream${c.reset}`);

  class UppercaseTransform extends Transform {
    _transform(chunk, encoding, callback) {
      this.push(chunk.toString().toUpperCase());
      callback();
    }
  }

  const input = 'hello world';
  const uppercase = new UppercaseTransform();

  let transformed = '';
  uppercase.on('data', chunk => transformed += chunk);
  uppercase.on('end', () => {
    console.log(`  Input:     "${input}"`);
    console.log(`  Uppercase: "${transformed}"`);
  });

  uppercase.write(input);
  uppercase.end();

  setTimeout(() => {
    // 4. Line-by-line processing
    console.log(`\n${c.bold}4. Line Processing Transform${c.reset}`);

    class LineSplitter extends Transform {
      constructor() {
        super({ objectMode: true });
        this.buffer = '';
      }

      _transform(chunk, encoding, callback) {
        this.buffer += chunk.toString();
        const lines = this.buffer.split('\n');
        this.buffer = lines.pop();

        for (const line of lines) {
          if (line.trim()) {
            this.push({ line: line.trim(), length: line.trim().length });
          }
        }
        callback();
      }

      _flush(callback) {
        if (this.buffer.trim()) {
          this.push({ line: this.buffer.trim(), length: this.buffer.trim().length });
        }
        callback();
      }
    }

    const textData = `First line
Second line is longer
Third
Fourth line here`;

    const splitter = new LineSplitter();
    const lines = [];
    splitter.on('data', line => lines.push(line));
    splitter.on('end', () => {
      console.log(`  Lines processed: ${lines.length}`);
      lines.forEach(l => console.log(`  ${c.dim}  "${l.line}" (${l.length} chars)${c.reset}`));
    });

    splitter.write(textData);
    splitter.end();

    setTimeout(() => {
      // 5. JSON stream parser
      console.log(`\n${c.bold}5. JSON Stream Parser${c.reset}`);

      class JSONParser extends Transform {
        constructor() {
          super({ objectMode: true });
        }

        _transform(chunk, encoding, callback) {
          try {
            const data = JSON.parse(chunk.toString());
            this.push(data);
            callback();
          } catch (e) {
            callback(new Error(`Invalid JSON: ${e.message}`));
          }
        }
      }

      const jsonParser = new JSONParser();
      const parsed = [];
      jsonParser.on('data', obj => parsed.push(obj));
      jsonParser.on('end', () => {
        console.log(`  Parsed ${parsed.length} JSON objects:`);
        parsed.forEach(obj => console.log(`  ${c.dim}  ${JSON.stringify(obj)}${c.reset}`));
      });

      jsonParser.write('{"name": "Alice", "age": 30}');
      jsonParser.write('{"name": "Bob", "age": 25}');
      jsonParser.end();

      setTimeout(() => {
        // 6. Aggregating stream
        console.log(`\n${c.bold}6. Aggregating Stream${c.reset}`);

        class Aggregator extends Writable {
          constructor() {
            super({ objectMode: true });
            this.sum = 0;
            this.count = 0;
            this.min = Infinity;
            this.max = -Infinity;
          }

          _write(chunk, encoding, callback) {
            const num = typeof chunk === 'number' ? chunk : chunk.value;
            this.sum += num;
            this.count++;
            this.min = Math.min(this.min, num);
            this.max = Math.max(this.max, num);
            callback();
          }

          getStats() {
            return {
              sum: this.sum,
              count: this.count,
              avg: this.count > 0 ? this.sum / this.count : 0,
              min: this.min,
              max: this.max,
            };
          }
        }

        const aggregator = new Aggregator();
        [10, 20, 30, 40, 50].forEach(n => aggregator.write({ value: n }));
        aggregator.end();

        const stats = aggregator.getStats();
        console.log(`  Sum: ${stats.sum}, Count: ${stats.count}, Avg: ${stats.avg}`);
        console.log(`  Min: ${stats.min}, Max: ${stats.max}`);

        // 7. PassThrough for monitoring
        console.log(`\n${c.bold}7. PassThrough Monitor${c.reset}`);

        class Monitor extends PassThrough {
          constructor(name) {
            super();
            this.name = name;
            this.bytesProcessed = 0;

            this.on('data', chunk => {
              this.bytesProcessed += chunk.length;
            });
          }
        }

        const monitor = new Monitor('data-flow');
        const output = [];
        monitor.on('data', chunk => output.push(chunk.toString()));
        monitor.on('end', () => {
          console.log(`  Passed through: "${output.join('')}"`);
          console.log(`  Bytes processed: ${monitor.bytesProcessed}`);
        });

        monitor.write('Hello ');
        monitor.write('World!');
        monitor.end();

        setTimeout(() => {
          // 8. Collecting stream
          console.log(`\n${c.bold}8. Collecting Stream${c.reset}`);

          class CollectingWritable extends Writable {
            constructor() {
              super();
              this.chunks = [];
            }

            _write(chunk, encoding, callback) {
              this.chunks.push(chunk.toString());
              callback();
            }

            getContent() {
              return this.chunks.join('');
            }
          }

          const collector = new CollectingWritable();
          collector.write('Line 1: Hello from streams!\n');
          collector.write('Line 2: This is written with streams.\n');
          collector.write('Line 3: Streams are powerful!\n');
          collector.end();

          const content = collector.getContent();
          console.log(`  Collected content:`);
          content.split('\n').filter(Boolean).forEach(line => {
            console.log(`  ${c.dim}  ${line}${c.reset}`);
          });

          console.log(`\n${c.green}${c.bold}Stream processing demo completed!${c.reset}\n`);
        }, 50);
      }, 50);
    }, 50);
  }, 50);
}, 100);
