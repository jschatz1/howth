/**
 * API Client Example
 *
 * Demonstrates:
 * - HTTP client (fetch)
 * - JSON API consumption
 * - Async/await patterns
 * - Error handling
 * - Response processing
 *
 * Run: howth run --native examples/api-client/client.js
 */

const http = require('http');

// Colors
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

// Simple fetch wrapper using http module
async function fetchJson(url) {
  return new Promise((resolve, reject) => {
    const req = http.get(url, (res) => {
      let data = '';
      res.on('data', chunk => data += chunk);
      res.on('end', () => {
        try {
          resolve({
            ok: res.statusCode >= 200 && res.statusCode < 300,
            status: res.statusCode,
            data: JSON.parse(data),
          });
        } catch (e) {
          reject(new Error(`Invalid JSON: ${e.message}`));
        }
      });
    });
    req.on('error', reject);
    req.setTimeout(10000, () => {
      req.destroy();
      reject(new Error('Request timeout'));
    });
  });
}

// Example API: JSONPlaceholder (free fake API)
const API_BASE = 'http://jsonplaceholder.typicode.com';

async function getUsers() {
  console.log(`${c.cyan}Fetching users...${c.reset}`);
  const res = await fetchJson(`${API_BASE}/users`);

  if (!res.ok) {
    throw new Error(`Failed to fetch users: ${res.status}`);
  }

  return res.data;
}

async function getPosts(userId) {
  console.log(`${c.cyan}Fetching posts for user ${userId}...${c.reset}`);
  const res = await fetchJson(`${API_BASE}/posts?userId=${userId}`);

  if (!res.ok) {
    throw new Error(`Failed to fetch posts: ${res.status}`);
  }

  return res.data;
}

async function getComments(postId) {
  console.log(`${c.cyan}Fetching comments for post ${postId}...${c.reset}`);
  const res = await fetchJson(`${API_BASE}/comments?postId=${postId}`);

  if (!res.ok) {
    throw new Error(`Failed to fetch comments: ${res.status}`);
  }

  return res.data;
}

async function getTodos(userId) {
  console.log(`${c.cyan}Fetching todos for user ${userId}...${c.reset}`);
  const res = await fetchJson(`${API_BASE}/todos?userId=${userId}`);

  if (!res.ok) {
    throw new Error(`Failed to fetch todos: ${res.status}`);
  }

  return res.data;
}

// Main demo
async function main() {
  console.log(`\n${c.bold}API Client Demo${c.reset}`);
  console.log(`${c.dim}Using JSONPlaceholder API${c.reset}\n`);

  try {
    // Get all users
    const users = await getUsers();
    console.log(`\n${c.green}Found ${users.length} users${c.reset}\n`);

    // Show first 3 users
    console.log(`${c.bold}Users:${c.reset}`);
    for (const user of users.slice(0, 3)) {
      console.log(`  ${c.blue}${user.name}${c.reset} (@${user.username})`);
      console.log(`    ${c.dim}Email: ${user.email}${c.reset}`);
      console.log(`    ${c.dim}Company: ${user.company.name}${c.reset}`);
    }

    // Get posts for first user
    const userId = users[0].id;
    const posts = await getPosts(userId);
    console.log(`\n${c.green}Found ${posts.length} posts for ${users[0].name}${c.reset}\n`);

    // Show first 3 posts
    console.log(`${c.bold}Recent Posts:${c.reset}`);
    for (const post of posts.slice(0, 3)) {
      console.log(`  ${c.yellow}${post.title.substring(0, 50)}...${c.reset}`);

      // Get comments for this post
      const comments = await getComments(post.id);
      console.log(`    ${c.dim}${comments.length} comments${c.reset}`);
    }

    // Get todos for first user
    const todos = await getTodos(userId);
    const completed = todos.filter(t => t.completed).length;
    console.log(`\n${c.green}Found ${todos.length} todos for ${users[0].name}${c.reset}`);
    console.log(`  ${c.green}✓ Completed: ${completed}${c.reset}`);
    console.log(`  ${c.yellow}○ Pending: ${todos.length - completed}${c.reset}`);

    console.log(`\n${c.bold}${c.green}API client demo completed successfully!${c.reset}\n`);

  } catch (err) {
    console.error(`\n${c.red}Error: ${err.message}${c.reset}`);
    process.exit(1);
  }
}

main();
