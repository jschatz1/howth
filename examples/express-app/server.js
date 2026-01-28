/**
 * Express.js Example
 *
 * Demonstrates Express running on the Howth runtime:
 * - Routing (GET, POST, PUT, DELETE)
 * - Middleware (logging, JSON parsing, static files)
 * - Template rendering
 * - Error handling
 * - API routes with JSON responses
 *
 * Run: cd examples/express-app && pnpm install && howth run server.js
 * Then visit: http://localhost:3000
 */

const express = require('express');
const path = require('path');
const fs = require('fs');

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

// Serve static files
app.use('/static', express.static(path.join(__dirname, 'public')));

// --- Simple template engine ---

function renderTemplate(name, data) {
  const templatePath = path.join(__dirname, 'views', `${name}.html`);
  let html = fs.readFileSync(templatePath, 'utf8');
  for (const [key, value] of Object.entries(data)) {
    html = html.replace(new RegExp(`{{\\s*${key}\\s*}}`, 'g'), value);
  }
  return html;
}

// --- In-memory data store ---

const todos = [
  { id: 1, title: 'Try Express on Howth', done: true },
  { id: 2, title: 'Build something cool', done: false },
  { id: 3, title: 'Read the docs', done: false },
];
let nextId = 4;

// --- Routes ---

// Home page
app.get('/', (req, res) => {
  const todoItems = todos
    .map(t => `<li class="${t.done ? 'done' : ''}">${t.title} ${t.done ? '✓' : ''}</li>`)
    .join('\n');
  res.send(renderTemplate('index', {
    title: 'Express on Howth',
    todoItems,
    todoCount: String(todos.length),
  }));
});

// About page
app.get('/about', (req, res) => {
  res.send(renderTemplate('about', {
    title: 'About',
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
  res.status(404).send(renderTemplate('error', {
    title: '404 Not Found',
    message: `The page ${req.url} was not found.`,
  }));
});

// Error handler
app.use((err, req, res, next) => {
  console.error('Server error:', err.stack || err);
  res.status(500).send(renderTemplate('error', {
    title: '500 Server Error',
    message: 'Something went wrong.',
  }));
});

// --- Start ---

app.listen(PORT, () => {
  console.log(`Express server running at http://localhost:${PORT}`);
  console.log('Routes:');
  console.log('  GET  /            - Home page');
  console.log('  GET  /about       - About page');
  console.log('  GET  /api/todos   - List todos');
  console.log('  POST /api/todos   - Create todo');
  console.log('  GET  /api/health  - Health check');
});
