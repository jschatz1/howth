/**
 * LRU Cache Example
 *
 * A Least Recently Used (LRU) cache implementation with:
 * - O(1) get/set operations
 * - Configurable max size
 * - TTL (time-to-live) support
 * - Cache statistics
 * - Event callbacks
 *
 * Run: howth run --native examples/lru-cache/cache.js
 */

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

/**
 * Doubly Linked List Node
 */
class Node {
  constructor(key, value, ttl = null) {
    this.key = key;
    this.value = value;
    this.prev = null;
    this.next = null;
    this.createdAt = Date.now();
    this.expiresAt = ttl ? Date.now() + ttl : null;
  }

  isExpired() {
    return this.expiresAt !== null && Date.now() > this.expiresAt;
  }
}

/**
 * LRU Cache
 */
class LRUCache {
  constructor(options = {}) {
    this.maxSize = options.maxSize || 100;
    this.defaultTTL = options.ttl || null; // milliseconds
    this.onEvict = options.onEvict || null;
    this.onHit = options.onHit || null;
    this.onMiss = options.onMiss || null;

    this.cache = new Map();
    this.head = null; // Most recently used
    this.tail = null; // Least recently used

    this.stats = {
      hits: 0,
      misses: 0,
      evictions: 0,
      sets: 0,
    };
  }

  // Get a value from cache
  get(key) {
    const node = this.cache.get(key);

    if (!node) {
      this.stats.misses++;
      if (this.onMiss) this.onMiss(key);
      return undefined;
    }

    // Check if expired
    if (node.isExpired()) {
      this.delete(key);
      this.stats.misses++;
      if (this.onMiss) this.onMiss(key);
      return undefined;
    }

    // Move to front (most recently used)
    this.moveToFront(node);

    this.stats.hits++;
    if (this.onHit) this.onHit(key, node.value);

    return node.value;
  }

  // Set a value in cache
  set(key, value, ttl = this.defaultTTL) {
    this.stats.sets++;

    // If key exists, update value and move to front
    if (this.cache.has(key)) {
      const node = this.cache.get(key);
      node.value = value;
      node.expiresAt = ttl ? Date.now() + ttl : null;
      this.moveToFront(node);
      return this;
    }

    // Create new node
    const node = new Node(key, value, ttl);
    this.cache.set(key, node);

    // Add to front of list
    this.addToFront(node);

    // Evict if over capacity
    while (this.cache.size > this.maxSize) {
      this.evictLRU();
    }

    return this;
  }

  // Check if key exists (doesn't update LRU order)
  has(key) {
    const node = this.cache.get(key);
    if (!node) return false;
    if (node.isExpired()) {
      this.delete(key);
      return false;
    }
    return true;
  }

  // Delete a key
  delete(key) {
    const node = this.cache.get(key);
    if (!node) return false;

    this.removeFromList(node);
    this.cache.delete(key);
    return true;
  }

  // Clear all entries
  clear() {
    this.cache.clear();
    this.head = null;
    this.tail = null;
  }

  // Get cache size
  get size() {
    return this.cache.size;
  }

  // Get all keys
  keys() {
    return [...this.cache.keys()];
  }

  // Get all values
  values() {
    return [...this.cache.values()].map(node => node.value);
  }

  // Get all entries
  entries() {
    return [...this.cache.entries()].map(([key, node]) => [key, node.value]);
  }

  // Peek at a value without updating LRU order
  peek(key) {
    const node = this.cache.get(key);
    if (!node || node.isExpired()) return undefined;
    return node.value;
  }

  // Move node to front of list
  moveToFront(node) {
    if (node === this.head) return;
    this.removeFromList(node);
    this.addToFront(node);
  }

  // Add node to front of list
  addToFront(node) {
    node.prev = null;
    node.next = this.head;

    if (this.head) {
      this.head.prev = node;
    }
    this.head = node;

    if (!this.tail) {
      this.tail = node;
    }
  }

  // Remove node from list
  removeFromList(node) {
    if (node.prev) {
      node.prev.next = node.next;
    } else {
      this.head = node.next;
    }

    if (node.next) {
      node.next.prev = node.prev;
    } else {
      this.tail = node.prev;
    }
  }

  // Evict least recently used item
  evictLRU() {
    if (!this.tail) return;

    const evicted = this.tail;
    this.removeFromList(evicted);
    this.cache.delete(evicted.key);

    this.stats.evictions++;
    if (this.onEvict) this.onEvict(evicted.key, evicted.value);
  }

  // Prune expired entries
  prune() {
    let pruned = 0;
    for (const [key, node] of this.cache) {
      if (node.isExpired()) {
        this.delete(key);
        pruned++;
      }
    }
    return pruned;
  }

  // Get cache statistics
  getStats() {
    const total = this.stats.hits + this.stats.misses;
    return {
      ...this.stats,
      size: this.cache.size,
      maxSize: this.maxSize,
      hitRate: total > 0 ? (this.stats.hits / total * 100).toFixed(2) + '%' : 'N/A',
    };
  }

  // Reset statistics
  resetStats() {
    this.stats = { hits: 0, misses: 0, evictions: 0, sets: 0 };
  }

  // Debug: dump cache state
  dump() {
    const items = [];
    let node = this.head;
    while (node) {
      items.push({
        key: node.key,
        value: node.value,
        expired: node.isExpired(),
      });
      node = node.next;
    }
    return items;
  }
}

/**
 * Memoize function with LRU cache
 */
function memoize(fn, options = {}) {
  const cache = new LRUCache(options);

  const memoized = function(...args) {
    const key = options.keyFn
      ? options.keyFn(...args)
      : JSON.stringify(args);

    if (cache.has(key)) {
      return cache.get(key);
    }

    const result = fn.apply(this, args);
    cache.set(key, result);
    return result;
  };

  memoized.cache = cache;
  memoized.clear = () => cache.clear();

  return memoized;
}

// Export for module use
if (typeof module !== 'undefined') {
  module.exports = { LRUCache, memoize };
}

// Demo
console.log(`\n${c.bold}${c.cyan}LRU Cache Demo${c.reset}\n`);

// Create cache with callbacks
const cache = new LRUCache({
  maxSize: 5,
  ttl: 5000, // 5 seconds default TTL
  onEvict: (key, value) => {
    console.log(`${c.yellow}  Evicted: ${key}${c.reset}`);
  },
  onHit: (key) => {
    console.log(`${c.green}  Hit: ${key}${c.reset}`);
  },
  onMiss: (key) => {
    console.log(`${c.red}  Miss: ${key}${c.reset}`);
  },
});

console.log(`${c.bold}1. Basic operations${c.reset}`);
cache.set('a', 1);
cache.set('b', 2);
cache.set('c', 3);
console.log(`  Set a=1, b=2, c=3`);
console.log(`  Get 'a':`, cache.get('a'));
console.log(`  Get 'b':`, cache.get('b'));
console.log(`  Cache size: ${cache.size}\n`);

console.log(`${c.bold}2. LRU eviction (maxSize=5)${c.reset}`);
cache.set('d', 4);
cache.set('e', 5);
console.log(`  Added d=4, e=5 (size: ${cache.size})`);
cache.set('f', 6); // This should evict 'c' (LRU)
console.log(`  Added f=6 - should evict LRU item`);
console.log(`  Cache keys: ${cache.keys().join(', ')}\n`);

console.log(`${c.bold}3. Access pattern affects eviction${c.reset}`);
cache.get('a'); // Move 'a' to front
cache.set('g', 7); // Should evict 'd' now (not 'a')
console.log(`  Accessed 'a', then added 'g'`);
console.log(`  Cache keys: ${cache.keys().join(', ')}\n`);

console.log(`${c.bold}4. TTL expiration${c.reset}`);
const shortCache = new LRUCache({ maxSize: 10 });
shortCache.set('temp', 'value', 100); // 100ms TTL
console.log(`  Set 'temp' with 100ms TTL`);
console.log(`  Immediate get:`, shortCache.get('temp'));

await new Promise(r => setTimeout(r, 150));
console.log(`  After 150ms:`, shortCache.get('temp'), '\n');

console.log(`${c.bold}5. Memoization${c.reset}`);
let computeCount = 0;
const expensiveCompute = memoize((n) => {
  computeCount++;
  console.log(`${c.dim}  Computing fibonacci(${n})...${c.reset}`);
  if (n <= 1) return n;
  return expensiveCompute(n - 1) + expensiveCompute(n - 2);
}, { maxSize: 50 });

console.log(`  fibonacci(10) =`, expensiveCompute(10));
console.log(`  fibonacci(10) again =`, expensiveCompute(10), '(cached)');
console.log(`  Total computations: ${computeCount}\n`);

console.log(`${c.bold}6. Cache statistics${c.reset}`);
const stats = cache.getStats();
console.log(`  Hits: ${stats.hits}`);
console.log(`  Misses: ${stats.misses}`);
console.log(`  Hit rate: ${stats.hitRate}`);
console.log(`  Evictions: ${stats.evictions}`);
console.log(`  Total sets: ${stats.sets}\n`);

console.log(`${c.bold}7. Cache dump${c.reset}`);
console.log(`  Current state (head=MRU, tail=LRU):`);
for (const item of cache.dump()) {
  console.log(`    ${item.key}: ${item.value}`);
}

console.log(`\n${c.green}${c.bold}LRU Cache demo completed!${c.reset}\n`);
