/**
 * Rate Limiter Example
 *
 * Demonstrates rate limiting patterns:
 * - Token bucket algorithm
 * - Sliding window
 * - Fixed window
 * - Per-IP limiting
 * - HTTP middleware
 *
 * Run: howth run --native examples/rate-limiter/limiter.js
 */

const http = require('http');
const url = require('url');

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  red: '\x1b[31m',
  dim: '\x1b[2m',
};

console.log(`\n${c.bold}${c.cyan}Rate Limiter Demo${c.reset}\n`);

/**
 * Token Bucket Rate Limiter
 * - Allows bursts up to bucket capacity
 * - Refills at constant rate
 */
class TokenBucket {
  constructor(options = {}) {
    this.capacity = options.capacity || 10;      // Max tokens
    this.refillRate = options.refillRate || 1;   // Tokens per second
    this.tokens = this.capacity;
    this.lastRefill = Date.now();
  }

  refill() {
    const now = Date.now();
    const elapsed = (now - this.lastRefill) / 1000;
    this.tokens = Math.min(this.capacity, this.tokens + elapsed * this.refillRate);
    this.lastRefill = now;
  }

  consume(tokens = 1) {
    this.refill();
    if (this.tokens >= tokens) {
      this.tokens -= tokens;
      return true;
    }
    return false;
  }

  getState() {
    this.refill();
    return {
      tokens: Math.floor(this.tokens),
      capacity: this.capacity,
      refillRate: this.refillRate,
    };
  }
}

/**
 * Sliding Window Rate Limiter
 * - Counts requests in sliding time window
 * - More accurate than fixed window
 */
class SlidingWindow {
  constructor(options = {}) {
    this.windowMs = options.windowMs || 60000;   // Window size in ms
    this.maxRequests = options.maxRequests || 60; // Max requests per window
    this.requests = [];
  }

  cleanup() {
    const cutoff = Date.now() - this.windowMs;
    this.requests = this.requests.filter(t => t > cutoff);
  }

  consume() {
    this.cleanup();
    if (this.requests.length < this.maxRequests) {
      this.requests.push(Date.now());
      return true;
    }
    return false;
  }

  getState() {
    this.cleanup();
    return {
      count: this.requests.length,
      maxRequests: this.maxRequests,
      windowMs: this.windowMs,
      resetIn: this.requests.length > 0
        ? Math.ceil((this.requests[0] + this.windowMs - Date.now()) / 1000)
        : 0,
    };
  }
}

/**
 * Fixed Window Rate Limiter
 * - Simpler but can allow 2x burst at window boundaries
 */
class FixedWindow {
  constructor(options = {}) {
    this.windowMs = options.windowMs || 60000;
    this.maxRequests = options.maxRequests || 60;
    this.currentWindow = this.getCurrentWindow();
    this.count = 0;
  }

  getCurrentWindow() {
    return Math.floor(Date.now() / this.windowMs);
  }

  consume() {
    const window = this.getCurrentWindow();
    if (window !== this.currentWindow) {
      this.currentWindow = window;
      this.count = 0;
    }

    if (this.count < this.maxRequests) {
      this.count++;
      return true;
    }
    return false;
  }

  getState() {
    const window = this.getCurrentWindow();
    if (window !== this.currentWindow) {
      this.currentWindow = window;
      this.count = 0;
    }

    return {
      count: this.count,
      maxRequests: this.maxRequests,
      windowMs: this.windowMs,
      resetIn: Math.ceil((this.windowMs - (Date.now() % this.windowMs)) / 1000),
    };
  }
}

/**
 * Per-Key Rate Limiter (e.g., per IP)
 */
class PerKeyLimiter {
  constructor(LimiterClass, options = {}) {
    this.LimiterClass = LimiterClass;
    this.options = options;
    this.limiters = new Map();
    this.cleanupInterval = setInterval(() => this.cleanup(), 60000);
  }

  getLimiter(key) {
    if (!this.limiters.has(key)) {
      this.limiters.set(key, {
        limiter: new this.LimiterClass(this.options),
        lastAccess: Date.now(),
      });
    }
    const entry = this.limiters.get(key);
    entry.lastAccess = Date.now();
    return entry.limiter;
  }

  consume(key) {
    return this.getLimiter(key).consume();
  }

  getState(key) {
    return this.getLimiter(key).getState();
  }

  cleanup() {
    const cutoff = Date.now() - 300000; // 5 minutes
    for (const [key, entry] of this.limiters) {
      if (entry.lastAccess < cutoff) {
        this.limiters.delete(key);
      }
    }
  }

  stop() {
    clearInterval(this.cleanupInterval);
  }
}

// Demo the limiters
console.log(`${c.bold}1. Token Bucket Demo${c.reset}`);
const bucket = new TokenBucket({ capacity: 5, refillRate: 2 });

for (let i = 0; i < 8; i++) {
  const allowed = bucket.consume();
  const state = bucket.getState();
  console.log(`  Request ${i + 1}: ${allowed ? c.green + '✓ allowed' : c.red + '✗ denied'}${c.reset} (${state.tokens}/${state.capacity} tokens)`);
}

console.log(`\n${c.bold}2. Sliding Window Demo${c.reset}`);
const sliding = new SlidingWindow({ windowMs: 1000, maxRequests: 3 });

for (let i = 0; i < 5; i++) {
  const allowed = sliding.consume();
  const state = sliding.getState();
  console.log(`  Request ${i + 1}: ${allowed ? c.green + '✓ allowed' : c.red + '✗ denied'}${c.reset} (${state.count}/${state.maxRequests} in window)`);
}

console.log(`\n${c.bold}3. Fixed Window Demo${c.reset}`);
const fixed = new FixedWindow({ windowMs: 1000, maxRequests: 3 });

for (let i = 0; i < 5; i++) {
  const allowed = fixed.consume();
  const state = fixed.getState();
  console.log(`  Request ${i + 1}: ${allowed ? c.green + '✓ allowed' : c.red + '✗ denied'}${c.reset} (${state.count}/${state.maxRequests}, resets in ${state.resetIn}s)`);
}

// HTTP Server with rate limiting
console.log(`\n${c.bold}4. HTTP Server with Rate Limiting${c.reset}`);

const PORT = process.env.PORT || 3000;
const ipLimiter = new PerKeyLimiter(TokenBucket, {
  capacity: 10,
  refillRate: 2,
});

const server = http.createServer((req, res) => {
  const ip = req.socket.remoteAddress || 'unknown';
  const parsedUrl = url.parse(req.url, true);

  // Rate limit check
  if (!ipLimiter.consume(ip)) {
    const state = ipLimiter.getState(ip);
    res.writeHead(429, {
      'Content-Type': 'application/json',
      'X-RateLimit-Limit': state.capacity,
      'X-RateLimit-Remaining': state.tokens,
      'Retry-After': Math.ceil((state.capacity - state.tokens) / state.refillRate),
    });
    res.end(JSON.stringify({
      error: 'Too Many Requests',
      message: 'Rate limit exceeded. Please slow down.',
      retryAfter: Math.ceil((state.capacity - state.tokens) / state.refillRate),
    }));
    console.log(`  ${c.red}429${c.reset} ${ip} ${req.url} (rate limited)`);
    return;
  }

  const state = ipLimiter.getState(ip);

  // Add rate limit headers
  res.setHeader('X-RateLimit-Limit', state.capacity);
  res.setHeader('X-RateLimit-Remaining', state.tokens);

  // Routes
  if (parsedUrl.pathname === '/') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      message: 'Rate Limiter Demo API',
      endpoints: [
        'GET / - This info',
        'GET /status - Your rate limit status',
        'GET /burst - Test burst requests',
      ],
      rateLimit: state,
    }, null, 2));
    console.log(`  ${c.green}200${c.reset} ${ip} ${req.url}`);
    return;
  }

  if (parsedUrl.pathname === '/status') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      ip,
      rateLimit: state,
    }));
    console.log(`  ${c.green}200${c.reset} ${ip} ${req.url}`);
    return;
  }

  if (parsedUrl.pathname === '/burst') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      message: 'Request successful!',
      timestamp: new Date().toISOString(),
      remaining: state.tokens,
    }));
    console.log(`  ${c.green}200${c.reset} ${ip} ${req.url} (${state.tokens} remaining)`);
    return;
  }

  res.writeHead(404, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({ error: 'Not Found' }));
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`\n  ${c.green}➜${c.reset}  Server: ${c.cyan}http://localhost:${PORT}${c.reset}`);
  console.log(`  ${c.dim}Rate limit: 10 requests, refills at 2/sec${c.reset}`);
  console.log(`\n${c.dim}Test with:${c.reset}`);
  console.log(`  curl http://localhost:${PORT}/status`);
  console.log(`  for i in {1..15}; do curl -s http://localhost:${PORT}/burst | jq -r '.remaining // .error'; done`);
  console.log(`\n${c.dim}Press Ctrl+C to stop${c.reset}\n`);
});

// Cleanup on exit
process.on('SIGINT', () => {
  ipLimiter.stop();
  server.close();
  process.exit(0);
});
