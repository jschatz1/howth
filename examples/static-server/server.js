/**
 * Static File Server Example
 *
 * Demonstrates:
 * - Serving static files
 * - MIME type detection
 * - Directory listing
 * - Security (path traversal prevention)
 * - Caching headers
 *
 * Run: howth run --native examples/static-server/server.js
 *      howth run --native examples/static-server/server.js ./public
 */

const http = require('http');
const fs = require('fs');
const path = require('path');
const url = require('url');

const PORT = process.env.PORT || 3002;
const ROOT = process.argv[2] || process.cwd();

// MIME types
const MIME_TYPES = {
  '.html': 'text/html',
  '.htm': 'text/html',
  '.css': 'text/css',
  '.js': 'application/javascript',
  '.mjs': 'application/javascript',
  '.json': 'application/json',
  '.txt': 'text/plain',
  '.md': 'text/markdown',
  '.xml': 'application/xml',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.ico': 'image/x-icon',
  '.webp': 'image/webp',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.ttf': 'font/ttf',
  '.otf': 'font/otf',
  '.pdf': 'application/pdf',
  '.zip': 'application/zip',
  '.mp3': 'audio/mpeg',
  '.mp4': 'video/mp4',
  '.webm': 'video/webm',
};

function getMimeType(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  return MIME_TYPES[ext] || 'application/octet-stream';
}

function formatSize(bytes) {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
}

function escapeHtml(str) {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function renderDirectoryListing(dirPath, urlPath, entries) {
  const rows = entries.map(entry => {
    const icon = entry.isDirectory ? '📁' : '📄';
    const name = entry.isDirectory ? entry.name + '/' : entry.name;
    const href = path.join(urlPath, entry.name);
    const size = entry.isDirectory ? '-' : formatSize(entry.size);
    const modified = new Date(entry.mtime).toLocaleDateString();

    return `
      <tr>
        <td>${icon} <a href="${escapeHtml(href)}">${escapeHtml(name)}</a></td>
        <td>${size}</td>
        <td>${modified}</td>
      </tr>
    `;
  }).join('');

  const parentLink = urlPath !== '/' ? `<a href="${path.dirname(urlPath)}">⬆️ Parent Directory</a>` : '';

  return `
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Index of ${escapeHtml(urlPath)}</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 2rem; }
    h1 { color: #333; border-bottom: 1px solid #ddd; padding-bottom: 0.5rem; }
    table { border-collapse: collapse; width: 100%; max-width: 800px; }
    th, td { padding: 0.5rem 1rem; text-align: left; border-bottom: 1px solid #eee; }
    th { background: #f5f5f5; }
    a { color: #0066cc; text-decoration: none; }
    a:hover { text-decoration: underline; }
    .parent { margin-bottom: 1rem; }
    footer { margin-top: 2rem; color: #666; font-size: 0.9rem; }
  </style>
</head>
<body>
  <h1>Index of ${escapeHtml(urlPath)}</h1>
  <div class="parent">${parentLink}</div>
  <table>
    <thead>
      <tr><th>Name</th><th>Size</th><th>Modified</th></tr>
    </thead>
    <tbody>
      ${rows}
    </tbody>
  </table>
  <footer>Served by Howth Static Server</footer>
</body>
</html>
  `;
}

const server = http.createServer((req, res) => {
  const parsedUrl = url.parse(req.url);
  let pathname = decodeURIComponent(parsedUrl.pathname);

  // Prevent path traversal attacks
  const safePath = path.normalize(pathname).replace(/^(\.\.[\/\\])+/, '');
  const filePath = path.join(ROOT, safePath);

  // Ensure we're still within ROOT
  if (!filePath.startsWith(path.resolve(ROOT))) {
    res.writeHead(403, { 'Content-Type': 'text/plain' });
    return res.end('403 Forbidden');
  }

  console.log(`${new Date().toISOString()} ${req.method} ${pathname}`);

  // Check if file exists
  if (!fs.existsSync(filePath)) {
    res.writeHead(404, { 'Content-Type': 'text/plain' });
    return res.end('404 Not Found');
  }

  const stat = fs.statSync(filePath);

  // Handle directories
  if (stat.isDirectory()) {
    // Try index.html first
    const indexPath = path.join(filePath, 'index.html');
    if (fs.existsSync(indexPath)) {
      const content = fs.readFileSync(indexPath);
      res.writeHead(200, {
        'Content-Type': 'text/html',
        'Content-Length': content.length,
      });
      return res.end(content);
    }

    // Show directory listing
    const entries = fs.readdirSync(filePath)
      .filter(name => !name.startsWith('.'))
      .map(name => {
        const entryPath = path.join(filePath, name);
        const entryStat = fs.statSync(entryPath);
        return {
          name,
          isDirectory: entryStat.isDirectory(),
          size: entryStat.size,
          mtime: entryStat.mtime,
        };
      })
      .sort((a, b) => {
        // Directories first, then alphabetical
        if (a.isDirectory && !b.isDirectory) return -1;
        if (!a.isDirectory && b.isDirectory) return 1;
        return a.name.localeCompare(b.name);
      });

    const html = renderDirectoryListing(filePath, pathname, entries);
    res.writeHead(200, { 'Content-Type': 'text/html' });
    return res.end(html);
  }

  // Serve file
  const mimeType = getMimeType(filePath);
  const content = fs.readFileSync(filePath);

  res.writeHead(200, {
    'Content-Type': mimeType,
    'Content-Length': content.length,
    'Cache-Control': 'public, max-age=3600',
    'Last-Modified': stat.mtime.toUTCString(),
  });
  res.end(content);
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`Static file server running at http://127.0.0.1:${PORT}/`);
  console.log(`Serving files from: ${path.resolve(ROOT)}`);
  console.log('Press Ctrl+C to stop');
});
