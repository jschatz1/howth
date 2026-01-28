/**
 * Proxy Server Example
 *
 * A simple HTTP proxy server that:
 * - Forwards requests to target servers
 * - Modifies headers
 * - Logs all requests
 * - Supports path rewriting
 * - Handles errors gracefully
 *
 * Run: howth run --native examples/proxy-server/proxy.js
 * Test: curl http://localhost:3080/api/users (proxies to jsonplaceholder)
 */

const http = require('http');
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

const PORT = process.env.PORT || 3080;

// Proxy configuration
const config = {
  // Route mappings: local path -> target
  routes: {
    '/api': {
      target: 'http://jsonplaceholder.typicode.com',
      pathRewrite: { '^/api': '' }, // Remove /api prefix
    },
    '/github': {
      target: 'http://api.github.com',
      pathRewrite: { '^/github': '' },
      headers: { 'User-Agent': 'Howth-Proxy/1.0' },
    },
    '/httpbin': {
      target: 'http://httpbin.org',
      pathRewrite: { '^/httpbin': '' },
    },
  },

  // Default headers to add to proxied requests
  defaultHeaders: {
    'X-Forwarded-By': 'Howth-Proxy',
  },

  // Enable request logging
  logging: true,

  // Request timeout in ms
  timeout: 30000,
};

// Request counter for logging
let requestId = 0;

// Log a request
function logRequest(id, method, path, status, duration, target) {
  if (!config.logging) return;

  const statusColor = status >= 500 ? c.red :
                      status >= 400 ? c.yellow :
                      status >= 300 ? c.cyan :
                      c.green;

  console.log(
    `${c.dim}[${id}]${c.reset} ` +
    `${c.bold}${method}${c.reset} ${path} ` +
    `${statusColor}${status}${c.reset} ` +
    `${c.dim}${duration}ms${c.reset} ` +
    `${c.dim}-> ${target}${c.reset}`
  );
}

// Rewrite path based on rules
function rewritePath(path, rules) {
  let rewritten = path;
  for (const [pattern, replacement] of Object.entries(rules)) {
    rewritten = rewritten.replace(new RegExp(pattern), replacement);
  }
  return rewritten;
}

// Find matching route
function findRoute(pathname) {
  for (const [prefix, routeConfig] of Object.entries(config.routes)) {
    if (pathname.startsWith(prefix)) {
      return { prefix, config: routeConfig };
    }
  }
  return null;
}

// Make HTTP request (simplified client)
function makeRequest(options) {
  return new Promise((resolve, reject) => {
    const req = http.request(options, (res) => {
      let data = '';
      res.on('data', chunk => data += chunk);
      res.on('end', () => {
        resolve({
          statusCode: res.statusCode,
          headers: res.headers,
          body: data,
        });
      });
    });

    req.on('error', reject);

    req.setTimeout(config.timeout, () => {
      req.destroy();
      reject(new Error('Request timeout'));
    });

    if (options.body) {
      req.write(options.body);
    }

    req.end();
  });
}

// Proxy request handler
async function handleProxy(req, res, route) {
  const id = ++requestId;
  const start = Date.now();

  const parsedUrl = url.parse(req.url, true);
  let targetPath = parsedUrl.pathname;

  // Apply path rewriting
  if (route.config.pathRewrite) {
    targetPath = rewritePath(targetPath, route.config.pathRewrite);
  }

  // Add query string
  if (parsedUrl.search) {
    targetPath += parsedUrl.search;
  }

  // Parse target URL
  const targetUrl = url.parse(route.config.target);

  // Build request options
  const options = {
    hostname: targetUrl.hostname,
    port: targetUrl.port || 80,
    path: targetPath,
    method: req.method,
    headers: {
      ...req.headers,
      ...config.defaultHeaders,
      ...route.config.headers,
      host: targetUrl.host, // Override host header
    },
  };

  // Read request body
  let body = '';
  req.on('data', chunk => body += chunk);

  await new Promise(resolve => req.on('end', resolve));

  if (body) {
    options.body = body;
  }

  try {
    const response = await makeRequest(options);
    const duration = Date.now() - start;

    // Log request
    logRequest(id, req.method, req.url, response.statusCode, duration, route.config.target + targetPath);

    // Copy response headers (excluding some)
    const skipHeaders = ['transfer-encoding', 'connection'];
    for (const [key, value] of Object.entries(response.headers)) {
      if (!skipHeaders.includes(key.toLowerCase())) {
        res.setHeader(key, value);
      }
    }

    // Add proxy headers
    res.setHeader('X-Proxy-By', 'Howth-Proxy');
    res.setHeader('X-Proxy-Time', `${duration}ms`);

    // Send response
    res.writeHead(response.statusCode);
    res.end(response.body);

  } catch (error) {
    const duration = Date.now() - start;
    logRequest(id, req.method, req.url, 502, duration, route.config.target);

    res.writeHead(502, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      error: 'Bad Gateway',
      message: error.message,
      target: route.config.target,
    }));
  }
}

// Create server
const server = http.createServer(async (req, res) => {
  const parsedUrl = url.parse(req.url);

  // Health check
  if (parsedUrl.pathname === '/health') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    return res.end(JSON.stringify({ status: 'ok', requests: requestId }));
  }

  // Proxy info
  if (parsedUrl.pathname === '/') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    return res.end(JSON.stringify({
      name: 'Howth Proxy Server',
      routes: Object.entries(config.routes).map(([prefix, cfg]) => ({
        prefix,
        target: cfg.target,
      })),
      endpoints: [
        'GET /health - Health check',
        'GET /api/* - Proxy to jsonplaceholder.typicode.com',
        'GET /github/* - Proxy to api.github.com',
        'GET /httpbin/* - Proxy to httpbin.org',
      ],
    }, null, 2));
  }

  // Find matching route
  const route = findRoute(parsedUrl.pathname);

  if (!route) {
    res.writeHead(404, { 'Content-Type': 'application/json' });
    return res.end(JSON.stringify({
      error: 'Not Found',
      message: `No proxy route configured for ${parsedUrl.pathname}`,
      availableRoutes: Object.keys(config.routes),
    }));
  }

  // Handle proxy
  await handleProxy(req, res, route);
});

// Start server
server.listen(PORT, '127.0.0.1', () => {
  console.log(`\n${c.bold}${c.cyan}Howth Proxy Server${c.reset}`);
  console.log(`${c.dim}Running at http://127.0.0.1:${PORT}${c.reset}\n`);

  console.log(`${c.bold}Configured routes:${c.reset}`);
  for (const [prefix, cfg] of Object.entries(config.routes)) {
    console.log(`  ${c.green}${prefix}/*${c.reset} -> ${c.blue}${cfg.target}${c.reset}`);
  }

  console.log(`\n${c.bold}Try these URLs:${c.reset}`);
  console.log(`  curl http://localhost:${PORT}/api/users`);
  console.log(`  curl http://localhost:${PORT}/api/posts/1`);
  console.log(`  curl http://localhost:${PORT}/httpbin/get`);
  console.log(`  curl http://localhost:${PORT}/health\n`);
});
