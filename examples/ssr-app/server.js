/**
 * Server-Side Rendering (SSR) Example
 *
 * A simple SSR framework demonstrating:
 * - Template rendering with components
 * - Reactive state hydration
 * - API routes
 * - Data fetching on server
 *
 * Run: howth run --native examples/ssr-app/server.js
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
  dim: '\x1b[2m',
};

const PORT = process.env.PORT || 3000;

// Simple component system
const components = {};

function defineComponent(name, render) {
  components[name] = render;
}

function renderComponent(name, props = {}) {
  if (!components[name]) {
    return `<!-- Component "${name}" not found -->`;
  }
  return components[name](props);
}

// HTML escaping
function escapeHtml(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// Template literal tag for HTML
function html(strings, ...values) {
  return strings.reduce((result, str, i) => {
    const value = values[i - 1];
    if (Array.isArray(value)) {
      return result + value.join('') + str;
    }
    return result + (value !== undefined ? value : '') + str;
  });
}

// Define components
defineComponent('Header', ({ title }) => html`
  <header style="background: linear-gradient(135deg, #667eea, #764ba2); color: white; padding: 2rem; margin: -1rem -1rem 1rem -1rem; border-radius: 8px 8px 0 0;">
    <h1 style="margin: 0;">${escapeHtml(title)}</h1>
    <p style="margin: 0.5rem 0 0 0; opacity: 0.9;">Server-Side Rendered with Howth</p>
  </header>
`);

defineComponent('Card', ({ title, children }) => html`
  <div style="background: white; border-radius: 8px; padding: 1.5rem; margin: 1rem 0; box-shadow: 0 2px 8px rgba(0,0,0,0.1);">
    <h3 style="margin-top: 0; color: #333;">${escapeHtml(title)}</h3>
    <div style="color: #666;">${children}</div>
  </div>
`);

defineComponent('Button', ({ text, onclick, style = '' }) => html`
  <button onclick="${onclick}" style="background: #667eea; color: white; border: none; padding: 0.75rem 1.5rem; border-radius: 6px; cursor: pointer; font-size: 1rem; ${style}">
    ${escapeHtml(text)}
  </button>
`);

defineComponent('UserList', ({ users }) => html`
  <ul style="list-style: none; padding: 0;">
    ${users.map(user => html`
      <li style="display: flex; align-items: center; padding: 0.75rem; border-bottom: 1px solid #eee;">
        <div style="width: 40px; height: 40px; background: #667eea; border-radius: 50%; display: flex; align-items: center; justify-content: center; color: white; font-weight: bold; margin-right: 1rem;">
          ${user.name[0]}
        </div>
        <div>
          <strong>${escapeHtml(user.name)}</strong>
          <br><small style="color: #888;">${escapeHtml(user.email)}</small>
        </div>
      </li>
    `).join('')}
  </ul>
`);

// Simulated database
const db = {
  users: [
    { id: 1, name: 'Alice Johnson', email: 'alice@example.com' },
    { id: 2, name: 'Bob Smith', email: 'bob@example.com' },
    { id: 3, name: 'Charlie Brown', email: 'charlie@example.com' },
    { id: 4, name: 'Diana Prince', email: 'diana@example.com' },
  ],
  posts: [
    { id: 1, title: 'Getting Started with SSR', author: 'Alice', views: 1234 },
    { id: 2, title: 'Building Fast Web Apps', author: 'Bob', views: 567 },
    { id: 3, title: 'The Future of JavaScript', author: 'Charlie', views: 890 },
  ],
};

// Page layouts
function renderLayout(title, content, initialState = {}) {
  return html`<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${escapeHtml(title)} - Howth SSR</title>
  <style>
    * { box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      max-width: 900px;
      margin: 0 auto;
      padding: 1rem;
      background: #f5f5f5;
      min-height: 100vh;
    }
    main {
      background: #fafafa;
      border-radius: 8px;
      padding: 1rem;
      box-shadow: 0 4px 16px rgba(0,0,0,0.1);
    }
    nav { margin-bottom: 1rem; }
    nav a {
      color: #667eea;
      text-decoration: none;
      margin-right: 1rem;
      font-weight: 500;
    }
    nav a:hover { text-decoration: underline; }
    .stats { display: flex; gap: 1rem; flex-wrap: wrap; }
    .stat {
      background: white;
      padding: 1rem 1.5rem;
      border-radius: 8px;
      box-shadow: 0 2px 8px rgba(0,0,0,0.1);
      text-align: center;
    }
    .stat-value { font-size: 2rem; font-weight: bold; color: #667eea; }
    .stat-label { color: #888; font-size: 0.9rem; }
  </style>
</head>
<body>
  <nav>
    <a href="/">Home</a>
    <a href="/users">Users</a>
    <a href="/posts">Posts</a>
    <a href="/about">About</a>
  </nav>
  <main>
    ${content}
  </main>
  <script>
    // Hydrate with server state
    window.__INITIAL_STATE__ = ${JSON.stringify(initialState)};
    console.log('Hydrated with state:', window.__INITIAL_STATE__);
  </script>
</body>
</html>`;
}

// Page handlers
const pages = {
  '/': () => {
    const stats = {
      users: db.users.length,
      posts: db.posts.length,
      totalViews: db.posts.reduce((sum, p) => sum + p.views, 0),
    };

    return renderLayout('Home', html`
      ${renderComponent('Header', { title: 'Welcome to Howth SSR' })}

      <div class="stats">
        <div class="stat">
          <div class="stat-value">${stats.users}</div>
          <div class="stat-label">Users</div>
        </div>
        <div class="stat">
          <div class="stat-value">${stats.posts}</div>
          <div class="stat-label">Posts</div>
        </div>
        <div class="stat">
          <div class="stat-value">${stats.totalViews.toLocaleString()}</div>
          <div class="stat-label">Total Views</div>
        </div>
      </div>

      ${renderComponent('Card', {
        title: 'Server-Side Rendering',
        children: html`
          <p>This page was rendered on the server at <strong>${new Date().toISOString()}</strong></p>
          <p>SSR benefits:</p>
          <ul>
            <li>Faster initial page load</li>
            <li>Better SEO</li>
            <li>Works without JavaScript</li>
            <li>Reduced client-side complexity</li>
          </ul>
        `
      })}

      ${renderComponent('Card', {
        title: 'Interactive Elements',
        children: html`
          <p>Counter: <span id="count">0</span></p>
          ${renderComponent('Button', { text: 'Increment', onclick: 'document.getElementById("count").textContent = parseInt(document.getElementById("count").textContent) + 1' })}
        `
      })}
    `, stats);
  },

  '/users': () => {
    return renderLayout('Users', html`
      ${renderComponent('Header', { title: 'User Directory' })}

      ${renderComponent('Card', {
        title: `All Users (${db.users.length})`,
        children: renderComponent('UserList', { users: db.users })
      })}
    `, { users: db.users });
  },

  '/posts': () => {
    return renderLayout('Posts', html`
      ${renderComponent('Header', { title: 'Blog Posts' })}

      ${db.posts.map(post => renderComponent('Card', {
        title: post.title,
        children: html`
          <p>By <strong>${escapeHtml(post.author)}</strong> · ${post.views.toLocaleString()} views</p>
        `
      })).join('')}
    `, { posts: db.posts });
  },

  '/about': () => {
    return renderLayout('About', html`
      ${renderComponent('Header', { title: 'About This App' })}

      ${renderComponent('Card', {
        title: 'Howth SSR Framework',
        children: html`
          <p>This is a demonstration of server-side rendering using Howth's native runtime.</p>
          <p>Features demonstrated:</p>
          <ul>
            <li>Component-based architecture</li>
            <li>Template rendering</li>
            <li>State hydration</li>
            <li>Route handling</li>
          </ul>
        `
      })}

      ${renderComponent('Card', {
        title: 'Technical Details',
        children: html`
          <table style="width: 100%; border-collapse: collapse;">
            <tr><td style="padding: 0.5rem; border-bottom: 1px solid #eee;"><strong>Runtime</strong></td><td style="padding: 0.5rem; border-bottom: 1px solid #eee;">Howth Native</td></tr>
            <tr><td style="padding: 0.5rem; border-bottom: 1px solid #eee;"><strong>Port</strong></td><td style="padding: 0.5rem; border-bottom: 1px solid #eee;">${PORT}</td></tr>
            <tr><td style="padding: 0.5rem; border-bottom: 1px solid #eee;"><strong>Node.js APIs</strong></td><td style="padding: 0.5rem; border-bottom: 1px solid #eee;">http, fs, path, url</td></tr>
            <tr><td style="padding: 0.5rem;"><strong>Components</strong></td><td style="padding: 0.5rem;">${Object.keys(components).length}</td></tr>
          </table>
        `
      })}
    `);
  },
};

// API routes
const api = {
  '/api/users': () => ({ users: db.users }),
  '/api/posts': () => ({ posts: db.posts }),
  '/api/stats': () => ({
    users: db.users.length,
    posts: db.posts.length,
    totalViews: db.posts.reduce((sum, p) => sum + p.views, 0),
    serverTime: new Date().toISOString(),
  }),
};

// Create server
const server = http.createServer((req, res) => {
  const parsedUrl = url.parse(req.url, true);
  const pathname = parsedUrl.pathname;

  console.log(`${c.dim}${req.method}${c.reset} ${pathname}`);

  // API routes
  if (pathname.startsWith('/api/')) {
    const handler = api[pathname];
    if (handler) {
      const data = handler();
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(data, null, 2));
      return;
    }
  }

  // Page routes
  const pageHandler = pages[pathname];
  if (pageHandler) {
    const html = pageHandler();
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(html);
    return;
  }

  // 404
  res.writeHead(404, { 'Content-Type': 'text/html' });
  res.end(renderLayout('Not Found', html`
    ${renderComponent('Header', { title: '404 - Page Not Found' })}
    ${renderComponent('Card', {
      title: 'Oops!',
      children: html`
        <p>The page <code>${escapeHtml(pathname)}</code> doesn't exist.</p>
        <p><a href="/">Go back home</a></p>
      `
    })}
  `));
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`\n${c.bold}${c.cyan}Howth SSR App${c.reset}`);
  console.log(`${c.dim}${'─'.repeat(40)}${c.reset}`);
  console.log(`  ${c.green}➜${c.reset}  Local:   ${c.cyan}http://localhost:${PORT}${c.reset}`);
  console.log(`\n${c.bold}Pages:${c.reset}`);
  Object.keys(pages).forEach(p => console.log(`  ${c.dim}•${c.reset} ${p}`));
  console.log(`\n${c.bold}API:${c.reset}`);
  Object.keys(api).forEach(p => console.log(`  ${c.dim}•${c.reset} ${p}`));
  console.log(`\n${c.dim}Press Ctrl+C to stop${c.reset}\n`);
});
