/**
 * Howth Playground Server
 *
 * A simple web-based code runner for howth.
 *
 * Run: howth run server.js
 * Open: http://localhost:3001
 */

const http = require('http');
const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');
const os = require('os');

const PORT = 3001;

// HTML page with Monaco editor
const html = `<!DOCTYPE html>
<html>
<head>
  <title>Howth Playground</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: system-ui, sans-serif; background: #1e1e1e; color: #fff; }
    .container { display: flex; height: 100vh; }
    .editor-pane { flex: 1; display: flex; flex-direction: column; border-right: 1px solid #333; }
    .output-pane { flex: 1; display: flex; flex-direction: column; }
    .header { padding: 12px 16px; background: #252526; border-bottom: 1px solid #333; display: flex; align-items: center; gap: 12px; }
    .header h1 { font-size: 14px; font-weight: 600; }
    .header select { background: #3c3c3c; color: #fff; border: 1px solid #555; padding: 4px 8px; border-radius: 4px; }
    #editor { flex: 1; }
    #output { flex: 1; background: #1e1e1e; padding: 12px; font-family: monospace; font-size: 13px; overflow: auto; white-space: pre-wrap; }
    .btn { background: #0e639c; color: #fff; border: none; padding: 6px 16px; border-radius: 4px; cursor: pointer; font-size: 13px; }
    .btn:hover { background: #1177bb; }
    .btn:disabled { opacity: 0.5; cursor: not-allowed; }
    .status { font-size: 12px; color: #888; }
    .error { color: #f48771; }
    .success { color: #89d185; }
  </style>
</head>
<body>
  <div class="container">
    <div class="editor-pane">
      <div class="header">
        <h1>Howth Playground</h1>
        <select id="examples">
          <option value="">-- Examples --</option>
          <option value="hello">Hello World</option>
          <option value="http">HTTP Server</option>
          <option value="fetch">Fetch API</option>
          <option value="crypto">Crypto</option>
          <option value="fs">File System</option>
        </select>
        <button class="btn" id="run">▶ Run</button>
        <span class="status" id="status"></span>
      </div>
      <div id="editor"></div>
    </div>
    <div class="output-pane">
      <div class="header">
        <h1>Output</h1>
        <button class="btn" id="clear">Clear</button>
      </div>
      <div id="output"></div>
    </div>
  </div>

  <script src="https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.45.0/min/vs/loader.min.js"></script>
  <script>
    const examples = {
      hello: 'console.log("Hello from Howth!");\\nconsole.log("Node.js API compatible runtime");\\nconsole.log("Written in Rust, powered by V8");',
      http: \`const http = require('http');

const server = http.createServer((req, res) => {
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({ message: 'Hello from Howth!', path: req.url }));
});

server.listen(3000, () => {
  console.log('Server running at http://localhost:3000');
});

// Auto-close after 5 seconds for demo
setTimeout(() => {
  server.close();
  console.log('Server closed');
}, 5000);\`,
      fetch: \`// Fetch API example
async function main() {
  console.log('Fetching data...');

  const res = await fetch('https://jsonplaceholder.typicode.com/todos/1');
  const data = await res.json();

  console.log('Response:', JSON.stringify(data, null, 2));
}

main().catch(console.error);\`,
      crypto: \`const crypto = require('crypto');

// Random bytes
const bytes = crypto.randomBytes(16);
console.log('Random bytes:', bytes.toString('hex'));

// UUID
console.log('UUID:', crypto.randomUUID());

// Hash
const hash = crypto.createHash('sha256').update('hello howth').digest('hex');
console.log('SHA-256:', hash);

// HMAC
const hmac = crypto.createHmac('sha256', 'secret').update('message').digest('hex');
console.log('HMAC:', hmac);\`,
      fs: \`const fs = require('fs');
const path = require('path');
const os = require('os');

// Write a file
const tmpFile = path.join(os.tmpdir(), 'howth-test.txt');
fs.writeFileSync(tmpFile, 'Hello from Howth!');
console.log('Wrote:', tmpFile);

// Read it back
const content = fs.readFileSync(tmpFile, 'utf8');
console.log('Read:', content);

// File stats
const stats = fs.statSync(tmpFile);
console.log('Size:', stats.size, 'bytes');

// Clean up
fs.unlinkSync(tmpFile);
console.log('Deleted file');\`
    };

    require.config({ paths: { vs: 'https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.45.0/min/vs' } });

    require(['vs/editor/editor.main'], function () {
      const editor = monaco.editor.create(document.getElementById('editor'), {
        value: examples.hello,
        language: 'javascript',
        theme: 'vs-dark',
        minimap: { enabled: false },
        fontSize: 14,
        lineNumbers: 'on',
        automaticLayout: true
      });

      const output = document.getElementById('output');
      const status = document.getElementById('status');
      const runBtn = document.getElementById('run');
      const examplesSelect = document.getElementById('examples');

      examplesSelect.addEventListener('change', (e) => {
        if (e.target.value && examples[e.target.value]) {
          editor.setValue(examples[e.target.value]);
        }
      });

      document.getElementById('clear').addEventListener('click', () => {
        output.textContent = '';
      });

      runBtn.addEventListener('click', async () => {
        const code = editor.getValue();
        output.textContent = '';
        status.textContent = 'Running...';
        status.className = 'status';
        runBtn.disabled = true;

        try {
          const res = await fetch('/run', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ code })
          });

          const reader = res.body.getReader();
          const decoder = new TextDecoder();

          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            output.textContent += decoder.decode(value);
            output.scrollTop = output.scrollHeight;
          }

          status.textContent = 'Done';
          status.className = 'status success';
        } catch (err) {
          output.textContent += '\\nError: ' + err.message;
          status.textContent = 'Error';
          status.className = 'status error';
        } finally {
          runBtn.disabled = false;
        }
      });
    });
  </script>
</body>
</html>`;

const server = http.createServer(async (req, res) => {
  if (req.method === 'GET' && req.url === '/') {
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(html);
    return;
  }

  if (req.method === 'POST' && req.url === '/run') {
    let body = '';
    for await (const chunk of req) {
      body += chunk;
    }

    const { code } = JSON.parse(body);

    // Write code to temp file
    const tmpDir = os.tmpdir();
    const tmpFile = path.join(tmpDir, `howth-playground-${Date.now()}.js`);
    fs.writeFileSync(tmpFile, code);

    res.writeHead(200, {
      'Content-Type': 'text/plain',
      'Transfer-Encoding': 'chunked'
    });

    // Run with howth (or node as fallback)
    let howthPath = process.env.HOWTH_BIN || 'howth';
    // Resolve relative paths
    if (howthPath.startsWith('./') || howthPath.startsWith('../')) {
      howthPath = path.resolve(process.cwd(), howthPath);
    }

    console.log('Running:', howthPath, 'run', tmpFile);

    let proc;
    try {
      proc = spawn(howthPath, ['run', tmpFile], {
        env: { ...process.env, NO_COLOR: '1' }
      });
    } catch (err) {
      res.write(`Spawn error: ${err.message}\n`);
      res.end();
      return;
    }

    proc.stdout.on('data', (data) => {
      console.log('stdout:', data.toString());
      res.write(data);
    });
    proc.stderr.on('data', (data) => {
      console.log('stderr:', data.toString());
      res.write(data);
    });

    proc.on('close', (code) => {
      console.log('Process closed with code:', code);
      // Clean up temp file
      try { fs.unlinkSync(tmpFile); } catch {}
      res.write(`\n[Process exited with code ${code}]`);
      res.end();
    });

    proc.on('error', (err) => {
      console.log('Process error:', err);
      res.write(`\nError: ${err.message}`);
      res.end();
    });

    // Timeout after 30 seconds
    setTimeout(() => {
      proc.kill();
    }, 30000);

    return;
  }

  res.writeHead(404);
  res.end('Not found');
});

server.listen(PORT, () => {
  console.log(`Howth Playground running at http://localhost:${PORT}`);
});
