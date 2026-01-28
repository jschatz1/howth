/**
 * Vite-like Development Server
 *
 * A simple dev server with:
 * - Static file serving
 * - ES module support
 * - Live reload via SSE
 * - File watching
 * - Source transforms (JSX-like syntax)
 *
 * Run: howth run --native examples/dev-server/server.js
 * Then visit: http://localhost:3000
 */

const http = require('http');
const fs = require('fs');
const path = require('path');
const url = require('url');

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  magenta: '\x1b[35m',
  dim: '\x1b[2m',
};

const PORT = process.env.PORT || 3000;
const ROOT = process.argv[2] || path.join(path.dirname(process.argv[1] || __filename), 'public');

// MIME types
const MIME_TYPES = {
  '.html': 'text/html',
  '.js': 'application/javascript',
  '.mjs': 'application/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
};

// SSE clients for live reload
const clients = new Set();

// Simple JSX-like transform (converts h() calls)
function transformJS(code, filePath) {
  // Add HMR client injection for HTML
  if (filePath.endsWith('.html')) {
    const hmrScript = `<script>
// Live reload client
const es = new EventSource('/__hmr');
es.onmessage = (e) => {
  if (e.data === 'reload') {
    console.log('[hmr] Reloading...');
    location.reload();
  }
};
es.onerror = () => console.log('[hmr] Connection lost, retrying...');
</script>`;
    return code.replace('</body>', `${hmrScript}\n</body>`);
  }

  // Transform simple JSX-like syntax: <div>text</div> -> h('div', null, 'text')
  // This is a very simplified transform for demo purposes
  if (filePath.endsWith('.jsx') || filePath.endsWith('.tsx')) {
    // Simple regex-based JSX transform (not production-ready!)
    code = code.replace(
      /<(\w+)([^>]*)>([^<]*)<\/\1>/g,
      (match, tag, attrs, children) => {
        const props = attrs.trim() ? `{${attrs.trim()}}` : 'null';
        return `h('${tag}', ${props}, '${children}')`;
      }
    );
  }

  return code;
}

// Serve a file
function serveFile(res, filePath) {
  const ext = path.extname(filePath);
  const contentType = MIME_TYPES[ext] || 'application/octet-stream';

  try {
    let content = fs.readFileSync(filePath, ext === '.png' || ext === '.jpg' || ext === '.ico' ? null : 'utf8');

    // Transform JS/HTML files
    if (typeof content === 'string') {
      content = transformJS(content, filePath);
    }

    res.writeHead(200, {
      'Content-Type': contentType,
      'Cache-Control': 'no-cache',
      'Access-Control-Allow-Origin': '*',
    });
    res.end(content);
    return true;
  } catch (e) {
    return false;
  }
}

// Create HTTP server
const server = http.createServer((req, res) => {
  const parsedUrl = url.parse(req.url, true);
  const pathname = parsedUrl.pathname;

  // HMR endpoint (Server-Sent Events)
  if (pathname === '/__hmr') {
    res.writeHead(200, {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      'Connection': 'keep-alive',
      'Access-Control-Allow-Origin': '*',
    });
    res.write('data: connected\n\n');

    clients.add(res);
    req.on('close', () => clients.delete(res));
    return;
  }

  // API endpoint example
  if (pathname === '/api/time') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ time: new Date().toISOString() }));
    return;
  }

  // Resolve file path
  let filePath = path.join(ROOT, pathname);

  // Try index.html for directories
  if (fs.existsSync(filePath) && fs.statSync(filePath).isDirectory()) {
    filePath = path.join(filePath, 'index.html');
  }

  // Try adding .html extension
  if (!fs.existsSync(filePath) && !path.extname(filePath)) {
    if (fs.existsSync(filePath + '.html')) {
      filePath += '.html';
    }
  }

  // Serve the file
  if (serveFile(res, filePath)) {
    const relPath = path.relative(ROOT, filePath);
    console.log(`${c.green}200${c.reset} ${c.dim}${req.method}${c.reset} ${pathname} ${c.dim}-> ${relPath}${c.reset}`);
    return;
  }

  // 404 - try serving index.html for SPA routing
  const indexPath = path.join(ROOT, 'index.html');
  if (fs.existsSync(indexPath) && pathname !== '/') {
    if (serveFile(res, indexPath)) {
      console.log(`${c.yellow}200${c.reset} ${c.dim}${req.method}${c.reset} ${pathname} ${c.dim}-> index.html (SPA)${c.reset}`);
      return;
    }
  }

  res.writeHead(404, { 'Content-Type': 'text/plain' });
  res.end('Not Found');
  console.log(`${c.red}404${c.reset} ${c.dim}${req.method}${c.reset} ${pathname}`);
});

// Notify clients to reload
function notifyReload() {
  for (const client of clients) {
    client.write('data: reload\n\n');
  }
}

// Simple file watcher
let watchTimeout = null;
function watchFiles(dir) {
  try {
    fs.watch(dir, { recursive: true }, (event, filename) => {
      if (watchTimeout) return;
      watchTimeout = setTimeout(() => {
        watchTimeout = null;
        console.log(`${c.magenta}[hmr]${c.reset} ${filename} changed, reloading...`);
        notifyReload();
      }, 100);
    });
  } catch (e) {
    console.log(`${c.yellow}Warning: File watching not available${c.reset}`);
  }
}

// Create default public directory with example files
function ensurePublicDir() {
  if (!fs.existsSync(ROOT)) {
    fs.mkdirSync(ROOT, { recursive: true });

    // Create index.html
    fs.writeFileSync(path.join(ROOT, 'index.html'), `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Howth Dev Server</title>
  <link rel="stylesheet" href="/style.css">
</head>
<body>
  <div id="app">
    <h1>Welcome to Howth Dev Server</h1>
    <p>Edit files in <code>public/</code> and see live reload in action!</p>
    <div id="counter"></div>
    <div id="time"></div>
  </div>
  <script type="module" src="/app.js"></script>
</body>
</html>`);

    // Create style.css
    fs.writeFileSync(path.join(ROOT, 'style.css'), `* {
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  max-width: 800px;
  margin: 0 auto;
  padding: 2rem;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  min-height: 100vh;
  color: white;
}

#app {
  background: rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(10px);
  border-radius: 16px;
  padding: 2rem;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.1);
}

h1 {
  margin-top: 0;
  font-size: 2.5rem;
}

code {
  background: rgba(0, 0, 0, 0.2);
  padding: 0.2em 0.5em;
  border-radius: 4px;
  font-size: 0.9em;
}

button {
  background: white;
  color: #667eea;
  border: none;
  padding: 0.75rem 1.5rem;
  border-radius: 8px;
  font-size: 1rem;
  cursor: pointer;
  transition: transform 0.2s, box-shadow 0.2s;
}

button:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
}

#counter {
  margin: 1.5rem 0;
  font-size: 1.5rem;
}

#time {
  margin-top: 1rem;
  opacity: 0.8;
}
`);

    // Create app.js
    fs.writeFileSync(path.join(ROOT, 'app.js'), `// Simple reactive counter app
let count = 0;

function render() {
  document.getElementById('counter').innerHTML = \`
    <p>Count: <strong>\${count}</strong></p>
    <button onclick="increment()">Increment</button>
    <button onclick="decrement()">Decrement</button>
  \`;
}

window.increment = () => { count++; render(); };
window.decrement = () => { count--; render(); };

// Fetch time from API
async function fetchTime() {
  try {
    const res = await fetch('/api/time');
    const data = await res.json();
    document.getElementById('time').innerHTML = \`
      <p>Server time: \${data.time}</p>
    \`;
  } catch (e) {
    console.error('Failed to fetch time:', e);
  }
}

// Initialize
render();
fetchTime();
setInterval(fetchTime, 5000);

console.log('App loaded! Try editing public/app.js or public/style.css');
`);

    console.log(`${c.green}Created${c.reset} example files in ${ROOT}`);
  }
}

// Start server
ensurePublicDir();
watchFiles(ROOT);

server.listen(PORT, '127.0.0.1', () => {
  console.log(`\n${c.bold}${c.cyan}Howth Dev Server${c.reset}`);
  console.log(`${c.dim}${'─'.repeat(40)}${c.reset}`);
  console.log(`  ${c.green}➜${c.reset}  Local:   ${c.cyan}http://localhost:${PORT}${c.reset}`);
  console.log(`  ${c.dim}➜${c.reset}  ${c.dim}Root:    ${ROOT}${c.reset}`);
  console.log(`  ${c.dim}➜${c.reset}  ${c.dim}HMR:     Enabled${c.reset}`);
  console.log(`\n${c.dim}Press Ctrl+C to stop${c.reset}\n`);
});
