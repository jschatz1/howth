/**
 * React SSR Example
 *
 * Demonstrates server-side rendered React running on the Howth runtime:
 * - React.createElement (no JSX, no build step)
 * - react-dom/server renderToString
 * - Express routing and middleware
 * - API routes with JSON responses
 *
 * Run: cd examples/react-app && howth install && howth run server.js
 * Then visit: http://localhost:3000
 */

const express = require('express');
const React = require('react');
const ReactDOMServer = require('react-dom/server');

const Layout = require('./components/Layout');
const HomePage = require('./components/HomePage');
const AboutPage = require('./components/AboutPage');

const app = express();
const PORT = process.env.PORT || 3000;

// --- Middleware ---

// Request logging
app.use((req, res, next) => {
  const start = Date.now();
  res.on('finish', () => {
    const ms = Date.now() - start;
    console.log(`${req.method} ${req.url} ${res.statusCode} ${ms}ms`);
  });
  next();
});

// Parse JSON bodies
app.use(express.json());

// Parse URL-encoded bodies
app.use(express.urlencoded({ extended: true }));

// --- In-memory data store ---

const todos = [
  { id: 1, title: 'Try React SSR on Howth', done: true },
  { id: 2, title: 'Build something cool', done: false },
  { id: 3, title: 'Read the docs', done: false },
];
let nextId = 4;

// --- Helper ---

function renderPage(component, props) {
  const page = React.createElement(Layout, null, React.createElement(component, props));
  return '<!DOCTYPE html>' + ReactDOMServer.renderToString(page);
}

// --- Routes ---

// Home page
app.get('/', (req, res) => {
  res.send(renderPage(HomePage, { todos }));
});

// About page
app.get('/about', (req, res) => {
  res.send(renderPage(AboutPage, {
    runtime: process.versions ? `Howth (V8 ${process.versions.v8 || 'unknown'})` : 'Howth',
    nodeVersion: process.version || 'unknown',
    platform: process.platform || 'unknown',
    arch: process.arch || 'unknown',
  }));
});

// --- API routes ---

// List todos
app.get('/api/todos', (req, res) => {
  res.json(todos);
});

// Get single todo
app.get('/api/todos/:id', (req, res) => {
  const todo = todos.find(t => t.id === parseInt(req.params.id));
  if (!todo) {
    return res.status(404).json({ error: 'Todo not found' });
  }
  res.json(todo);
});

// Create todo
app.post('/api/todos', (req, res) => {
  const { title } = req.body;
  if (!title) {
    return res.status(400).json({ error: 'Title is required' });
  }
  const todo = { id: nextId++, title, done: false };
  todos.push(todo);
  res.status(201).json(todo);
});

// Update todo
app.put('/api/todos/:id', (req, res) => {
  const todo = todos.find(t => t.id === parseInt(req.params.id));
  if (!todo) {
    return res.status(404).json({ error: 'Todo not found' });
  }
  if (req.body.title !== undefined) todo.title = req.body.title;
  if (req.body.done !== undefined) todo.done = req.body.done;
  res.json(todo);
});

// Delete todo
app.delete('/api/todos/:id', (req, res) => {
  const idx = todos.findIndex(t => t.id === parseInt(req.params.id));
  if (idx === -1) {
    return res.status(404).json({ error: 'Todo not found' });
  }
  const [removed] = todos.splice(idx, 1);
  res.json(removed);
});

// Health check
app.get('/api/health', (req, res) => {
  res.json({
    status: 'ok',
    runtime: 'howth',
    uptime: process.uptime(),
    timestamp: new Date().toISOString(),
  });
});

// --- Error handling ---

// 404 handler
app.use((req, res) => {
  const errorContent = React.createElement('div', { style: { textAlign: 'center' } },
    React.createElement('h1', { style: { color: '#dc2626', fontSize: '2rem', marginBottom: '1rem' } }, '404 Not Found'),
    React.createElement('p', { style: { color: '#666', marginBottom: '2rem' } }, `The page ${req.url} was not found.`),
    React.createElement('a', { href: '/', style: { color: '#4f46e5', textDecoration: 'none' } }, 'Back to Home')
  );
  const page = React.createElement(Layout, null, errorContent);
  res.status(404).send('<!DOCTYPE html>' + ReactDOMServer.renderToString(page));
});

// Error handler
app.use((err, req, res, next) => {
  console.error('Server error:', err.stack || err);
  const html = '<!DOCTYPE html>' + ReactDOMServer.renderToString(
    React.createElement(Layout, null,
      React.createElement('div', { style: { textAlign: 'center' } },
        React.createElement('h1', { style: { color: '#dc2626', fontSize: '2rem', marginBottom: '1rem' } }, '500 Server Error'),
        React.createElement('p', { style: { color: '#666', marginBottom: '2rem' } }, 'Something went wrong.'),
        React.createElement('a', { href: '/', style: { color: '#4f46e5', textDecoration: 'none' } }, 'Back to Home')
      )
    )
  );
  res.status(500).send(html);
});

// --- Start ---

app.listen(PORT, () => {
  console.log(`React SSR server running at http://localhost:${PORT}`);
  console.log('Routes:');
  console.log('  GET  /            - Home page (SSR)');
  console.log('  GET  /about       - About page (SSR)');
  console.log('  GET  /api/todos   - List todos');
  console.log('  POST /api/todos   - Create todo');
  console.log('  GET  /api/health  - Health check');
});
