/**
 * Static Site Generator
 *
 * A simple SSG that:
 * - Processes markdown files
 * - Applies layouts/templates
 * - Generates HTML output
 * - Copies static assets
 * - Creates a sitemap
 *
 * Run: howth run --native examples/static-site/build.js
 * Output: examples/static-site/dist/
 */

const fs = require('fs');
const path = require('path');

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

const ROOT = path.dirname(process.argv[1] || __filename);
const CONTENT_DIR = path.join(ROOT, 'content');
const OUTPUT_DIR = path.join(ROOT, 'dist');
const STATIC_DIR = path.join(ROOT, 'static');

console.log(`\n${c.bold}${c.cyan}Howth Static Site Generator${c.reset}\n`);

// Site configuration
const config = {
  title: 'My Howth Site',
  description: 'A static site built with Howth',
  baseUrl: 'https://example.com',
  author: 'Howth User',
};

// Simple markdown parser (basic implementation)
function parseMarkdown(content) {
  let html = content;

  // Extract frontmatter
  const frontmatterMatch = html.match(/^---\n([\s\S]*?)\n---\n/);
  let frontmatter = {};
  if (frontmatterMatch) {
    html = html.slice(frontmatterMatch[0].length);
    frontmatterMatch[1].split('\n').forEach(line => {
      const [key, ...rest] = line.split(':');
      if (key && rest.length) {
        frontmatter[key.trim()] = rest.join(':').trim();
      }
    });
  }

  // Headers
  html = html.replace(/^### (.+)$/gm, '<h3>$1</h3>');
  html = html.replace(/^## (.+)$/gm, '<h2>$1</h2>');
  html = html.replace(/^# (.+)$/gm, '<h1>$1</h1>');

  // Bold and italic
  html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>');

  // Code
  html = html.replace(/`([^`]+)`/g, '<code>$1</code>');

  // Links
  html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');

  // Lists
  html = html.replace(/^- (.+)$/gm, '<li>$1</li>');
  html = html.replace(/(<li>.*<\/li>\n?)+/g, '<ul>$&</ul>');

  // Paragraphs
  html = html.split('\n\n').map(p => {
    p = p.trim();
    if (!p) return '';
    if (p.startsWith('<')) return p;
    return `<p>${p}</p>`;
  }).join('\n');

  return { html, frontmatter };
}

// HTML template
function renderPage(content, meta = {}) {
  const title = meta.title ? `${meta.title} - ${config.title}` : config.title;
  const description = meta.description || config.description;

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="description" content="${description}">
  <meta name="author" content="${config.author}">
  <title>${title}</title>
  <style>
    :root {
      --primary: #667eea;
      --primary-dark: #5a67d8;
      --text: #333;
      --text-light: #666;
      --bg: #fff;
      --bg-alt: #f7f7f7;
      --border: #e2e8f0;
    }
    * { box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      line-height: 1.6;
      color: var(--text);
      max-width: 800px;
      margin: 0 auto;
      padding: 2rem;
      background: var(--bg);
    }
    header {
      border-bottom: 1px solid var(--border);
      padding-bottom: 1rem;
      margin-bottom: 2rem;
    }
    header h1 { margin: 0; }
    header nav { margin-top: 1rem; }
    header nav a {
      color: var(--primary);
      text-decoration: none;
      margin-right: 1.5rem;
      font-weight: 500;
    }
    header nav a:hover { text-decoration: underline; }
    main { min-height: 60vh; }
    article h1 { color: var(--primary); }
    article h2 { margin-top: 2rem; }
    article code {
      background: var(--bg-alt);
      padding: 0.2em 0.4em;
      border-radius: 4px;
      font-size: 0.9em;
    }
    article pre {
      background: var(--bg-alt);
      padding: 1rem;
      border-radius: 8px;
      overflow-x: auto;
    }
    article ul { padding-left: 1.5rem; }
    article a { color: var(--primary); }
    .meta {
      color: var(--text-light);
      font-size: 0.9rem;
      margin-bottom: 2rem;
    }
    footer {
      border-top: 1px solid var(--border);
      padding-top: 1rem;
      margin-top: 3rem;
      color: var(--text-light);
      font-size: 0.9rem;
    }
  </style>
</head>
<body>
  <header>
    <h1><a href="/" style="color: inherit; text-decoration: none;">${config.title}</a></h1>
    <nav>
      <a href="/">Home</a>
      <a href="/about.html">About</a>
      <a href="/blog.html">Blog</a>
    </nav>
  </header>
  <main>
    <article>
      ${meta.title ? `<h1>${meta.title}</h1>` : ''}
      ${meta.date ? `<p class="meta">Published on ${meta.date}${meta.author ? ` by ${meta.author}` : ''}</p>` : ''}
      ${content}
    </article>
  </main>
  <footer>
    <p>&copy; ${new Date().getFullYear()} ${config.author}. Built with Howth Static Site Generator.</p>
  </footer>
</body>
</html>`;
}

// Ensure directories exist
function ensureDir(dir) {
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
}

// Create sample content if it doesn't exist
function createSampleContent() {
  ensureDir(CONTENT_DIR);
  ensureDir(STATIC_DIR);

  const files = [
    {
      name: 'index.md',
      content: `---
title: Welcome
---

# Welcome to My Site

This is a **static site** built with Howth's static site generator.

## Features

- Markdown processing
- Frontmatter support
- Template layouts
- Fast builds

## Getting Started

Edit the files in \`content/\` and run the build script again.

[Learn more about Howth →](https://github.com)
`
    },
    {
      name: 'about.md',
      content: `---
title: About
description: Learn more about this site
---

# About This Site

This site was generated using **Howth**, a fast JavaScript runtime.

## The Stack

- **Runtime**: Howth Native
- **Build Tool**: Custom SSG
- **Styling**: Vanilla CSS

## Why Static Sites?

Static sites are:

- **Fast** - No server-side rendering
- **Secure** - No database to hack
- **Cheap** - Host anywhere for free
- **Reliable** - No moving parts
`
    },
    {
      name: 'blog.md',
      content: `---
title: Blog
---

# Blog Posts

## Recent Articles

- [Getting Started with Howth](/posts/getting-started.html)
- [Building Fast Web Apps](/posts/fast-web-apps.html)
- [The Future of JavaScript](/posts/future-of-js.html)
`
    },
  ];

  // Create posts directory
  ensureDir(path.join(CONTENT_DIR, 'posts'));

  const posts = [
    {
      name: 'posts/getting-started.md',
      content: `---
title: Getting Started with Howth
date: 2024-01-15
author: Alice
---

Learn how to get started with Howth, the fast JavaScript runtime.

## Installation

First, build Howth from source:

\`\`\`bash
cargo build --features native-runtime -p fastnode-cli
\`\`\`

## Running Your First Script

Create a file called \`hello.js\`:

\`\`\`javascript
console.log('Hello, Howth!');
\`\`\`

Then run it:

\`\`\`bash
howth run --native hello.js
\`\`\`

That's it! You're now running JavaScript with Howth.
`
    },
    {
      name: 'posts/fast-web-apps.md',
      content: `---
title: Building Fast Web Apps
date: 2024-01-10
author: Bob
---

Speed matters. Here's how to build fast web applications.

## Key Principles

1. **Minimize JavaScript** - Only ship what you need
2. **Optimize Images** - Use modern formats like WebP
3. **Cache Aggressively** - Set proper cache headers
4. **Use CDNs** - Distribute your static assets

## Performance Metrics

Focus on these Core Web Vitals:

- **LCP** - Largest Contentful Paint
- **FID** - First Input Delay
- **CLS** - Cumulative Layout Shift
`
    },
    {
      name: 'posts/future-of-js.md',
      content: `---
title: The Future of JavaScript
date: 2024-01-05
author: Charlie
---

What does the future hold for JavaScript?

## Trends to Watch

- **Edge Computing** - Running JS closer to users
- **WebAssembly** - High-performance alternatives
- **Server Components** - New rendering paradigms
- **Native Runtimes** - Alternatives to Node.js (like Howth!)

## Conclusion

JavaScript continues to evolve. Stay curious and keep learning!
`
    },
  ];

  // Write files if they don't exist
  [...files, ...posts].forEach(({ name, content }) => {
    const filePath = path.join(CONTENT_DIR, name);
    if (!fs.existsSync(filePath)) {
      fs.writeFileSync(filePath, content);
      console.log(`${c.green}Created${c.reset} ${name}`);
    }
  });
}

// Process all markdown files
function processContent() {
  const pages = [];

  function processDir(dir, outputSubDir = '') {
    const entries = fs.readdirSync(dir);

    for (const entry of entries) {
      const inputPath = path.join(dir, entry);
      const stat = fs.statSync(inputPath);

      if (stat.isDirectory()) {
        processDir(inputPath, path.join(outputSubDir, entry));
      } else if (entry.endsWith('.md')) {
        const content = fs.readFileSync(inputPath, 'utf8');
        const { html, frontmatter } = parseMarkdown(content);
        const outputName = entry.replace('.md', '.html');
        const outputPath = path.join(OUTPUT_DIR, outputSubDir, outputName);

        ensureDir(path.dirname(outputPath));

        const fullHtml = renderPage(html, frontmatter);
        fs.writeFileSync(outputPath, fullHtml);

        const relativePath = path.join(outputSubDir, outputName);
        pages.push({
          path: '/' + relativePath,
          title: frontmatter.title || entry,
          date: frontmatter.date,
        });

        console.log(`${c.blue}Built${c.reset} ${relativePath}`);
      }
    }
  }

  processDir(CONTENT_DIR);
  return pages;
}

// Copy static assets
function copyStatic() {
  if (!fs.existsSync(STATIC_DIR)) return;

  function copyDir(src, dest) {
    ensureDir(dest);
    const entries = fs.readdirSync(src);

    for (const entry of entries) {
      const srcPath = path.join(src, entry);
      const destPath = path.join(dest, entry);
      const stat = fs.statSync(srcPath);

      if (stat.isDirectory()) {
        copyDir(srcPath, destPath);
      } else {
        fs.copyFileSync(srcPath, destPath);
        console.log(`${c.dim}Copied${c.reset} ${entry}`);
      }
    }
  }

  copyDir(STATIC_DIR, OUTPUT_DIR);
}

// Generate sitemap
function generateSitemap(pages) {
  const sitemap = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${pages.map(page => `  <url>
    <loc>${config.baseUrl}${page.path}</loc>
    ${page.date ? `<lastmod>${page.date}</lastmod>` : ''}
  </url>`).join('\n')}
</urlset>`;

  fs.writeFileSync(path.join(OUTPUT_DIR, 'sitemap.xml'), sitemap);
  console.log(`${c.green}Generated${c.reset} sitemap.xml`);
}

// Build the site
function build() {
  const start = Date.now();

  // Clean output directory
  if (fs.existsSync(OUTPUT_DIR)) {
    fs.rmSync(OUTPUT_DIR, { recursive: true });
  }
  ensureDir(OUTPUT_DIR);

  // Create sample content if needed
  createSampleContent();

  // Process content
  console.log(`\n${c.bold}Processing content...${c.reset}`);
  const pages = processContent();

  // Copy static files
  console.log(`\n${c.bold}Copying static files...${c.reset}`);
  copyStatic();

  // Generate sitemap
  console.log(`\n${c.bold}Generating sitemap...${c.reset}`);
  generateSitemap(pages);

  const duration = Date.now() - start;

  console.log(`\n${c.green}${c.bold}Build completed!${c.reset}`);
  console.log(`${c.dim}${'─'.repeat(40)}${c.reset}`);
  console.log(`  Pages:    ${pages.length}`);
  console.log(`  Output:   ${OUTPUT_DIR}`);
  console.log(`  Time:     ${duration}ms`);
  console.log(`\n${c.dim}Preview with: howth run --native examples/dev-server/server.js ${OUTPUT_DIR}${c.reset}\n`);
}

build();
