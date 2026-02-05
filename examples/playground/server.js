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

const PORT = process.env.PORT || 3001;

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
    #preview { flex: 1; background: #fff; border: none; display: none; }
    .btn { background: #0e639c; color: #fff; border: none; padding: 6px 16px; border-radius: 4px; cursor: pointer; font-size: 13px; }
    .btn:hover { background: #1177bb; }
    .btn:disabled { opacity: 0.5; cursor: not-allowed; }
    .btn.active { background: #1177bb; }
    .status { font-size: 12px; color: #888; }
    .error { color: #f48771; }
    .success { color: #89d185; }
    .tabs { display: flex; gap: 4px; }
    .tab { background: #3c3c3c; color: #888; border: none; padding: 4px 12px; border-radius: 4px 4px 0 0; cursor: pointer; font-size: 12px; }
    .tab:hover { color: #fff; }
    .tab.active { background: #1e1e1e; color: #fff; }
  </style>
</head>
<body>
  <div class="container">
    <div class="editor-pane">
      <div class="header">
        <h1>Howth Playground</h1>
        <select id="examples">
          <option value="">-- Examples --</option>
          <optgroup label="Console">
            <option value="hello">Hello World</option>
            <option value="fetch">Fetch API</option>
            <option value="crypto">Crypto</option>
            <option value="fs">File System</option>
          </optgroup>
          <optgroup label="HTML Output">
            <option value="html-basic">Basic HTML</option>
            <option value="html-styled">Styled Page</option>
            <option value="html-interactive">Interactive (with JS)</option>
            <option value="react-ssr">React SSR</option>
            <option value="svg">SVG Graphics</option>
            <option value="markdown">Markdown to HTML</option>
          </optgroup>
        </select>
        <button class="btn" id="run">▶ Run</button>
        <span class="status" id="status"></span>
      </div>
      <div id="editor"></div>
    </div>
    <div class="output-pane">
      <div class="header">
        <div class="tabs">
          <button class="tab active" data-tab="console">Console</button>
          <button class="tab" data-tab="preview">Preview</button>
        </div>
        <button class="btn" id="clear">Clear</button>
      </div>
      <div id="output"></div>
      <iframe id="preview" sandbox="allow-scripts"></iframe>
    </div>
  </div>

  <script src="https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.45.0/min/vs/loader.min.js"></script>
  <script>
    const examples = {
      // Console examples
      hello: 'console.log("Hello from Howth!");\\nconsole.log("Node.js API compatible runtime");\\nconsole.log("Written in Rust, powered by V8");',

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
console.log('Random bytes:', crypto.randomBytes(16).toString('hex'));

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
console.log('Deleted file');\`,

      // HTML output examples
      'html-basic': \`// Basic HTML output - just print HTML to stdout
const html = \\\`<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>Hello from Howth</title>
</head>
<body>
  <h1>Hello from Howth!</h1>
  <p>This HTML was generated by a script running on the Howth runtime.</p>
  <p>Current time: \\\${new Date().toISOString()}</p>
</body>
</html>\\\`;

console.log(html);\`,

      'html-styled': \`// Styled HTML page
const html = \\\`<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: system-ui, sans-serif;
      background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .card {
      background: white;
      padding: 2rem 3rem;
      border-radius: 12px;
      box-shadow: 0 20px 60px rgba(0,0,0,0.3);
      text-align: center;
    }
    h1 { color: #333; margin-bottom: 0.5rem; }
    p { color: #666; }
    .badge {
      display: inline-block;
      background: #667eea;
      color: white;
      padding: 4px 12px;
      border-radius: 20px;
      font-size: 12px;
      margin-top: 1rem;
    }
  </style>
</head>
<body>
  <div class="card">
    <h1>Howth Runtime</h1>
    <p>Fast JavaScript execution powered by V8</p>
    <span class="badge">Built with Rust</span>
  </div>
</body>
</html>\\\`;

console.log(html);\`,

      'html-interactive': \`// Interactive HTML with JavaScript
const html = \\\`<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <style>
    body { font-family: system-ui; padding: 2rem; background: #f5f5f5; }
    .counter {
      background: white;
      padding: 2rem;
      border-radius: 8px;
      box-shadow: 0 2px 8px rgba(0,0,0,0.1);
      text-align: center;
      max-width: 300px;
      margin: 0 auto;
    }
    h1 { font-size: 3rem; color: #333; margin: 1rem 0; }
    button {
      background: #0066cc;
      color: white;
      border: none;
      padding: 10px 24px;
      border-radius: 6px;
      font-size: 16px;
      cursor: pointer;
      margin: 0 4px;
    }
    button:hover { background: #0052a3; }
    button.secondary { background: #666; }
    button.secondary:hover { background: #555; }
  </style>
</head>
<body>
  <div class="counter">
    <p>Count:</p>
    <h1 id="count">0</h1>
    <button class="secondary" onclick="decrement()">-</button>
    <button onclick="increment()">+</button>
  </div>
  <script>
    let count = 0;
    function increment() {
      count++;
      document.getElementById('count').textContent = count;
    }
    function decrement() {
      count--;
      document.getElementById('count').textContent = count;
    }
  </script>
</body>
</html>\\\`;

console.log(html);\`,

      'react-ssr': \`// React Server-Side Rendering
// Note: In a real app you'd use react-dom/server

// Simple JSX-like function
function h(tag, props, ...children) {
  const attrs = props ? Object.entries(props)
    .map(([k, v]) => \\\` \\\${k}="\\\${v}"\\\`)
    .join('') : '';
  const inner = children.flat().join('');
  return \\\`<\\\${tag}\\\${attrs}>\\\${inner}</\\\${tag}>\\\`;
}

// Components
function Header({ title }) {
  return h('header', { style: 'background:#0066cc;color:white;padding:1rem;' },
    h('h1', null, title)
  );
}

function TodoItem({ text, done }) {
  const style = done ? 'text-decoration:line-through;color:#999;' : '';
  return h('li', { style: \\\`padding:8px 0;border-bottom:1px solid #eee;\\\${style}\\\` }, text);
}

function TodoList({ items }) {
  return h('ul', { style: 'list-style:none;padding:0;' },
    ...items.map(item => TodoItem(item))
  );
}

function App() {
  const todos = [
    { text: 'Learn Howth', done: true },
    { text: 'Build something cool', done: false },
    { text: 'Ship it!', done: false },
  ];

  return h('div', { style: 'font-family:system-ui;max-width:400px;margin:0 auto;' },
    Header({ title: 'My Todos' }),
    h('main', { style: 'padding:1rem;' },
      TodoList({ items: todos }),
      h('p', { style: 'color:#666;font-size:14px;margin-top:1rem;' },
        \\\`\\\${todos.filter(t => t.done).length} of \\\${todos.length} completed\\\`
      )
    )
  );
}

// Render to HTML
const html = \\\`<!DOCTYPE html>
<html>
<head><meta charset="UTF-8"><title>React SSR</title></head>
<body>\\\${App()}</body>
</html>\\\`;

console.log(html);\`,

      svg: \`// SVG Graphics Generation
const width = 400;
const height = 300;

// Generate some data points
const points = [];
for (let i = 0; i < 10; i++) {
  points.push({
    x: 40 + i * 35,
    y: 250 - Math.floor(Math.random() * 200)
  });
}

// Build path
const pathD = points.map((p, i) =>
  (i === 0 ? 'M' : 'L') + p.x + ',' + p.y
).join(' ');

const html = \\\`<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <style>
    body { font-family: system-ui; padding: 2rem; background: #1a1a2e; }
    svg { display: block; margin: 0 auto; }
  </style>
</head>
<body>
  <svg width="\${width}" height="\${height}" viewBox="0 0 \${width} \${height}">
    <!-- Background -->
    <rect width="\${width}" height="\${height}" fill="#16213e"/>

    <!-- Grid lines -->
    \${[50, 100, 150, 200, 250].map(y =>
      \\\`<line x1="30" y1="\${y}" x2="\${width-20}" y2="\${y}" stroke="#1a1a4e" stroke-width="1"/>\\\`
    ).join('\\n    ')}

    <!-- Line chart -->
    <path d="\${pathD}" fill="none" stroke="#e94560" stroke-width="3" stroke-linecap="round"/>

    <!-- Data points -->
    \${points.map(p =>
      \\\`<circle cx="\${p.x}" cy="\${p.y}" r="6" fill="#e94560"/>
      <circle cx="\${p.x}" cy="\${p.y}" r="3" fill="#fff"/>\\\`
    ).join('\\n    ')}

    <!-- Title -->
    <text x="\${width/2}" y="30" text-anchor="middle" fill="#eee" font-size="16" font-weight="bold">
      Random Data Chart
    </text>
  </svg>
</body>
</html>\\\`;

console.log(html);\`,

      markdown: \`// Markdown to HTML converter
function markdown(text) {
  return text
    // Headers
    .replace(/^### (.*$)/gm, '<h3>$1</h3>')
    .replace(/^## (.*$)/gm, '<h2>$1</h2>')
    .replace(/^# (.*$)/gm, '<h1>$1</h1>')
    // Bold and italic
    .replace(/\\*\\*(.*)\\*\\*/g, '<strong>$1</strong>')
    .replace(/\\*(.*)\\*/g, '<em>$1</em>')
    // Code
    .replace(/\\\`([^\\\`]+)\\\`/g, '<code style="background:#f0f0f0;padding:2px 6px;border-radius:3px;">$1</code>')
    // Links
    .replace(/\\[([^\\]]+)\\]\\(([^)]+)\\)/g, '<a href="$2">$1</a>')
    // Lists
    .replace(/^- (.*$)/gm, '<li>$1</li>')
    // Paragraphs
    .replace(/^(?!<[hl]|<li)(.+)$/gm, '<p>$1</p>')
    // Wrap lists
    .replace(/(<li>.*<\\/li>\\n?)+/g, '<ul>$&</ul>');
}

const doc = \\\`# Welcome to Howth

Howth is a **fast** JavaScript runtime written in *Rust*.

## Features

- Native V8 engine
- Node.js API compatibility
- Built-in TypeScript support

## Quick Start

Run your first script with \\\\\\\`howth run script.js\\\\\\\`

Check out [the documentation](https://howth.run) for more info.
\\\`;

const html = \\\`<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <style>
    body {
      font-family: system-ui;
      max-width: 600px;
      margin: 2rem auto;
      padding: 0 1rem;
      line-height: 1.6;
      color: #333;
    }
    h1 { border-bottom: 2px solid #eee; padding-bottom: 0.5rem; }
    h2 { color: #666; margin-top: 2rem; }
    ul { padding-left: 1.5rem; }
    li { margin: 0.5rem 0; }
    a { color: #0066cc; }
    code { font-family: 'SF Mono', Monaco, monospace; }
  </style>
</head>
<body>
\${markdown(doc)}
</body>
</html>\\\`;

console.log(html);\`
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
      const preview = document.getElementById('preview');
      const status = document.getElementById('status');
      const runBtn = document.getElementById('run');
      const examplesSelect = document.getElementById('examples');
      const tabs = document.querySelectorAll('.tab');

      let currentTab = 'console';
      let lastOutput = '';

      // Tab switching
      tabs.forEach(tab => {
        tab.addEventListener('click', () => {
          currentTab = tab.dataset.tab;
          tabs.forEach(t => t.classList.remove('active'));
          tab.classList.add('active');

          if (currentTab === 'console') {
            output.style.display = 'block';
            preview.style.display = 'none';
          } else {
            output.style.display = 'none';
            preview.style.display = 'block';
            // If output looks like HTML, render it
            if (lastOutput.trim().startsWith('<!DOCTYPE') || lastOutput.trim().startsWith('<html')) {
              preview.srcdoc = lastOutput;
            }
          }
        });
      });

      examplesSelect.addEventListener('change', (e) => {
        if (e.target.value && examples[e.target.value]) {
          editor.setValue(examples[e.target.value]);
        }
      });

      document.getElementById('clear').addEventListener('click', () => {
        output.textContent = '';
        preview.srcdoc = '';
        lastOutput = '';
      });

      runBtn.addEventListener('click', async () => {
        const code = editor.getValue();
        output.textContent = '';
        preview.srcdoc = '';
        lastOutput = '';
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
            const text = decoder.decode(value);
            lastOutput += text;
            output.textContent += text;
            output.scrollTop = output.scrollHeight;
          }

          status.textContent = 'Done';
          status.className = 'status success';

          // Auto-switch to preview if output is HTML
          const trimmed = lastOutput.trim();
          if (trimmed.startsWith('<!DOCTYPE') || trimmed.startsWith('<html')) {
            // Remove the [Process exited...] line for preview
            const htmlOnly = lastOutput.replace(/\\n\\[Process exited.*\\]$/, '');
            preview.srcdoc = htmlOnly;
            // Switch to preview tab
            tabs.forEach(t => t.classList.remove('active'));
            document.querySelector('[data-tab="preview"]').classList.add('active');
            currentTab = 'preview';
            output.style.display = 'none';
            preview.style.display = 'block';
          }
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

    proc.stdout.on('data', (data) => res.write(data));
    proc.stderr.on('data', (data) => res.write(data));

    proc.on('close', (code) => {
      try { fs.unlinkSync(tmpFile); } catch {}
      res.write(`\n[Process exited with code ${code}]`);
      res.end();
    });

    proc.on('error', (err) => {
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
