/**
 * TODO REST API Example
 *
 * Demonstrates:
 * - RESTful API design
 * - In-memory data store
 * - JSON request/response handling
 * - CRUD operations
 * - Error handling
 *
 * Run: howth run --native examples/todo-api/server.js
 *
 * Test with curl:
 *   curl http://localhost:3001/todos
 *   curl -X POST -H "Content-Type: application/json" -d '{"title":"Buy milk"}' http://localhost:3001/todos
 *   curl -X PUT -H "Content-Type: application/json" -d '{"completed":true}' http://localhost:3001/todos/1
 *   curl -X DELETE http://localhost:3001/todos/1
 */

const http = require('http');
const url = require('url');

const PORT = process.env.PORT || 3001;

// In-memory store
let todos = [
  { id: 1, title: 'Learn Howth', completed: false, createdAt: new Date().toISOString() },
  { id: 2, title: 'Build something awesome', completed: false, createdAt: new Date().toISOString() },
];
let nextId = 3;

// Helpers
function jsonResponse(res, status, data) {
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type',
  });
  res.end(JSON.stringify(data, null, 2));
}

function parseBody(req) {
  return new Promise((resolve, reject) => {
    let body = '';
    req.on('data', chunk => body += chunk);
    req.on('end', () => {
      try {
        resolve(body ? JSON.parse(body) : {});
      } catch (e) {
        reject(new Error('Invalid JSON'));
      }
    });
    req.on('error', reject);
  });
}

// Route handlers
const handlers = {
  // List all todos
  'GET /todos': async (req, res) => {
    jsonResponse(res, 200, {
      todos,
      count: todos.length,
      completed: todos.filter(t => t.completed).length,
    });
  },

  // Get single todo
  'GET /todos/:id': async (req, res, params) => {
    const todo = todos.find(t => t.id === parseInt(params.id));
    if (!todo) {
      return jsonResponse(res, 404, { error: 'Todo not found' });
    }
    jsonResponse(res, 200, todo);
  },

  // Create todo
  'POST /todos': async (req, res) => {
    const body = await parseBody(req);
    if (!body.title) {
      return jsonResponse(res, 400, { error: 'Title is required' });
    }

    const todo = {
      id: nextId++,
      title: body.title,
      completed: false,
      createdAt: new Date().toISOString(),
    };
    todos.push(todo);

    console.log(`Created todo: ${todo.title}`);
    jsonResponse(res, 201, todo);
  },

  // Update todo
  'PUT /todos/:id': async (req, res, params) => {
    const todo = todos.find(t => t.id === parseInt(params.id));
    if (!todo) {
      return jsonResponse(res, 404, { error: 'Todo not found' });
    }

    const body = await parseBody(req);
    if (body.title !== undefined) todo.title = body.title;
    if (body.completed !== undefined) todo.completed = body.completed;
    todo.updatedAt = new Date().toISOString();

    console.log(`Updated todo ${todo.id}: ${todo.title}`);
    jsonResponse(res, 200, todo);
  },

  // Delete todo
  'DELETE /todos/:id': async (req, res, params) => {
    const index = todos.findIndex(t => t.id === parseInt(params.id));
    if (index === -1) {
      return jsonResponse(res, 404, { error: 'Todo not found' });
    }

    const [deleted] = todos.splice(index, 1);
    console.log(`Deleted todo ${deleted.id}: ${deleted.title}`);
    jsonResponse(res, 200, { message: 'Deleted', todo: deleted });
  },

  // Clear completed
  'DELETE /todos/completed': async (req, res) => {
    const before = todos.length;
    todos = todos.filter(t => !t.completed);
    const cleared = before - todos.length;

    console.log(`Cleared ${cleared} completed todos`);
    jsonResponse(res, 200, { message: `Cleared ${cleared} completed todos` });
  },
};

// Simple router with path params
function matchRoute(method, pathname) {
  // Try exact match first
  const exact = `${method} ${pathname}`;
  if (handlers[exact]) {
    return { handler: handlers[exact], params: {} };
  }

  // Try parameterized routes
  for (const [route, handler] of Object.entries(handlers)) {
    const [routeMethod, routePath] = route.split(' ');
    if (routeMethod !== method) continue;

    const routeParts = routePath.split('/');
    const pathParts = pathname.split('/');

    if (routeParts.length !== pathParts.length) continue;

    const params = {};
    let match = true;

    for (let i = 0; i < routeParts.length; i++) {
      if (routeParts[i].startsWith(':')) {
        params[routeParts[i].slice(1)] = pathParts[i];
      } else if (routeParts[i] !== pathParts[i]) {
        match = false;
        break;
      }
    }

    if (match) {
      return { handler, params };
    }
  }

  return null;
}

// Server
const server = http.createServer(async (req, res) => {
  const parsedUrl = url.parse(req.url, true);
  const method = req.method;
  const pathname = parsedUrl.pathname;

  console.log(`${new Date().toISOString()} ${method} ${pathname}`);

  // Handle CORS preflight
  if (method === 'OPTIONS') {
    res.writeHead(204, {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type',
    });
    return res.end();
  }

  // Route request
  const route = matchRoute(method, pathname);
  if (route) {
    try {
      await route.handler(req, res, route.params);
    } catch (err) {
      console.error('Error:', err.message);
      jsonResponse(res, 500, { error: err.message });
    }
  } else {
    jsonResponse(res, 404, { error: 'Not Found', path: pathname });
  }
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`TODO API running at http://127.0.0.1:${PORT}/`);
  console.log('\nEndpoints:');
  console.log('  GET    /todos          - List all todos');
  console.log('  GET    /todos/:id      - Get a todo');
  console.log('  POST   /todos          - Create a todo');
  console.log('  PUT    /todos/:id      - Update a todo');
  console.log('  DELETE /todos/:id      - Delete a todo');
  console.log('  DELETE /todos/completed - Clear completed todos');
  console.log('\nPress Ctrl+C to stop');
});
