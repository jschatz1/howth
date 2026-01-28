/**
 * Markdown Processor Example
 *
 * A simple markdown to HTML converter that:
 * - Handles common markdown syntax
 * - Supports code blocks with syntax hints
 * - Generates table of contents
 * - Extracts frontmatter
 * - Can process files or strings
 *
 * Run: howth run --native examples/markdown/md.js
 *      howth run --native examples/markdown/md.js -- README.md
 */

const fs = require('fs');
const path = require('path');

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
};

/**
 * Markdown Parser
 */
class MarkdownParser {
  constructor(options = {}) {
    this.options = {
      gfm: true,           // GitHub Flavored Markdown
      tables: true,        // Table support
      breaks: false,       // Convert \n to <br>
      headerIds: true,     // Add IDs to headers
      ...options,
    };
    this.toc = [];
  }

  // Parse markdown to HTML
  parse(markdown) {
    this.toc = [];
    let html = markdown;

    // Extract frontmatter
    const frontmatter = this.extractFrontmatter(html);
    if (frontmatter.content) {
      html = frontmatter.content;
    }

    // Process in order (block elements first)
    html = this.processCodeBlocks(html);
    html = this.processBlockquotes(html);
    html = this.processLists(html);
    html = this.processTables(html);
    html = this.processHeaders(html);
    html = this.processHorizontalRules(html);
    html = this.processParagraphs(html);

    // Inline elements
    html = this.processInlineCode(html);
    html = this.processBold(html);
    html = this.processItalic(html);
    html = this.processStrikethrough(html);
    html = this.processLinks(html);
    html = this.processImages(html);

    // Line breaks
    if (this.options.breaks) {
      html = html.replace(/\n/g, '<br>\n');
    }

    return {
      html,
      toc: this.toc,
      frontmatter: frontmatter.data,
    };
  }

  // Extract YAML frontmatter
  extractFrontmatter(markdown) {
    const match = markdown.match(/^---\n([\s\S]*?)\n---\n/);
    if (!match) return { data: null, content: markdown };

    const data = {};
    const lines = match[1].split('\n');
    for (const line of lines) {
      const [key, ...rest] = line.split(':');
      if (key && rest.length) {
        data[key.trim()] = rest.join(':').trim();
      }
    }

    return {
      data,
      content: markdown.slice(match[0].length),
    };
  }

  // Generate slug for header IDs
  slugify(text) {
    return text
      .toLowerCase()
      .replace(/<[^>]+>/g, '')
      .replace(/[^\w\s-]/g, '')
      .replace(/\s+/g, '-')
      .replace(/-+/g, '-')
      .trim();
  }

  // Process headers (# ## ### etc.)
  processHeaders(html) {
    return html.replace(/^(#{1,6})\s+(.+)$/gm, (match, hashes, content) => {
      const level = hashes.length;
      const text = this.processInline(content);
      const slug = this.slugify(content);

      this.toc.push({ level, text: content, slug });

      const id = this.options.headerIds ? ` id="${slug}"` : '';
      return `<h${level}${id}>${text}</h${level}>`;
    });
  }

  // Process code blocks (``` or indented)
  processCodeBlocks(html) {
    // Fenced code blocks
    html = html.replace(/```(\w*)\n([\s\S]*?)```/g, (match, lang, code) => {
      const escaped = this.escapeHtml(code.trim());
      const langClass = lang ? ` class="language-${lang}"` : '';
      return `<pre><code${langClass}>${escaped}</code></pre>`;
    });

    return html;
  }

  // Process inline code
  processInlineCode(html) {
    return html.replace(/`([^`]+)`/g, (match, code) => {
      return `<code>${this.escapeHtml(code)}</code>`;
    });
  }

  // Process blockquotes
  processBlockquotes(html) {
    const lines = html.split('\n');
    const result = [];
    let inBlockquote = false;
    let blockquoteContent = [];

    for (const line of lines) {
      if (line.startsWith('> ')) {
        inBlockquote = true;
        blockquoteContent.push(line.slice(2));
      } else if (inBlockquote && line.startsWith('>')) {
        blockquoteContent.push(line.slice(1).trim());
      } else {
        if (inBlockquote) {
          result.push(`<blockquote>${blockquoteContent.join('\n')}</blockquote>`);
          blockquoteContent = [];
          inBlockquote = false;
        }
        result.push(line);
      }
    }

    if (inBlockquote) {
      result.push(`<blockquote>${blockquoteContent.join('\n')}</blockquote>`);
    }

    return result.join('\n');
  }

  // Process unordered and ordered lists
  processLists(html) {
    // Unordered lists
    html = html.replace(/^([ \t]*[-*+][ \t]+.+\n?)+/gm, (match) => {
      const items = match.trim().split('\n').map(line => {
        const content = line.replace(/^[ \t]*[-*+][ \t]+/, '');
        return `<li>${this.processInline(content)}</li>`;
      });
      return `<ul>\n${items.join('\n')}\n</ul>`;
    });

    // Ordered lists
    html = html.replace(/^([ \t]*\d+\.[ \t]+.+\n?)+/gm, (match) => {
      const items = match.trim().split('\n').map(line => {
        const content = line.replace(/^[ \t]*\d+\.[ \t]+/, '');
        return `<li>${this.processInline(content)}</li>`;
      });
      return `<ol>\n${items.join('\n')}\n</ol>`;
    });

    return html;
  }

  // Process tables (GFM style)
  processTables(html) {
    if (!this.options.tables) return html;

    return html.replace(/^\|(.+)\|\n\|[-:| ]+\|\n((?:\|.+\|\n?)+)/gm, (match, header, body) => {
      // Parse header
      const headers = header.split('|').map(h => h.trim()).filter(Boolean);
      const headerHtml = headers.map(h => `<th>${this.processInline(h)}</th>`).join('');

      // Parse body
      const rows = body.trim().split('\n').map(row => {
        const cells = row.split('|').map(c => c.trim()).filter(Boolean);
        return `<tr>${cells.map(c => `<td>${this.processInline(c)}</td>`).join('')}</tr>`;
      });

      return `<table>\n<thead><tr>${headerHtml}</tr></thead>\n<tbody>\n${rows.join('\n')}\n</tbody>\n</table>`;
    });
  }

  // Process horizontal rules
  processHorizontalRules(html) {
    return html.replace(/^[-*_]{3,}$/gm, '<hr>');
  }

  // Process paragraphs
  processParagraphs(html) {
    // Split by double newlines
    const blocks = html.split(/\n\n+/);

    return blocks.map(block => {
      block = block.trim();
      // Skip if already wrapped in block element
      if (/^<(h[1-6]|p|div|ul|ol|li|blockquote|pre|table|hr)/i.test(block)) {
        return block;
      }
      if (!block) return '';
      return `<p>${this.processInline(block)}</p>`;
    }).join('\n\n');
  }

  // Process bold text
  processBold(html) {
    html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    html = html.replace(/__([^_]+)__/g, '<strong>$1</strong>');
    return html;
  }

  // Process italic text
  processItalic(html) {
    html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>');
    html = html.replace(/_([^_]+)_/g, '<em>$1</em>');
    return html;
  }

  // Process strikethrough (GFM)
  processStrikethrough(html) {
    if (!this.options.gfm) return html;
    return html.replace(/~~([^~]+)~~/g, '<del>$1</del>');
  }

  // Process links
  processLinks(html) {
    // [text](url "title")
    html = html.replace(/\[([^\]]+)\]\(([^)]+?)(?:\s+"([^"]*)")?\)/g, (match, text, url, title) => {
      const titleAttr = title ? ` title="${title}"` : '';
      return `<a href="${url}"${titleAttr}>${text}</a>`;
    });

    // Auto-link URLs
    if (this.options.gfm) {
      html = html.replace(/(^|[^"'])((https?:\/\/)[^\s<]+)/g, (match, prefix, url) => {
        return `${prefix}<a href="${url}">${url}</a>`;
      });
    }

    return html;
  }

  // Process images
  processImages(html) {
    return html.replace(/!\[([^\]]*)\]\(([^)]+?)(?:\s+"([^"]*)")?\)/g, (match, alt, src, title) => {
      const titleAttr = title ? ` title="${title}"` : '';
      return `<img src="${src}" alt="${alt}"${titleAttr}>`;
    });
  }

  // Process inline elements
  processInline(text) {
    text = this.processInlineCode(text);
    text = this.processBold(text);
    text = this.processItalic(text);
    text = this.processStrikethrough(text);
    text = this.processLinks(text);
    text = this.processImages(text);
    return text;
  }

  // Escape HTML entities
  escapeHtml(text) {
    return text
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  // Generate table of contents HTML
  generateToc() {
    if (this.toc.length === 0) return '';

    const items = this.toc.map(item => {
      const indent = '  '.repeat(item.level - 1);
      return `${indent}<li><a href="#${item.slug}">${item.text}</a></li>`;
    });

    return `<nav class="toc">\n<ul>\n${items.join('\n')}\n</ul>\n</nav>`;
  }
}

// Export for module use
if (typeof module !== 'undefined') {
  module.exports = { MarkdownParser };
}

// Demo
console.log(`\n${c.bold}${c.cyan}Markdown Processor Demo${c.reset}\n`);

// Sample markdown
const sampleMarkdown = `---
title: Sample Document
author: Howth
date: 2024-01-15
---

# Welcome to Markdown

This is a **bold** statement and this is *italic*.

## Features

Here's what we support:

- Unordered lists
- With multiple items
- And **bold** text inside

### Ordered Lists

1. First item
2. Second item
3. Third item

## Code Examples

Inline \`code\` looks like this.

\`\`\`javascript
function hello() {
  console.log("Hello, World!");
}
\`\`\`

## Links and Images

Visit [Howth](https://github.com) for more info.

![Alt text](image.png "Image title")

## Tables

| Name | Age | Role |
|------|-----|------|
| Alice | 30 | Admin |
| Bob | 25 | User |

## Blockquotes

> This is a blockquote.
> It can span multiple lines.

---

## Conclusion

That's all for now! ~~deleted text~~ and regular text.

Check out https://example.com for auto-linked URLs.
`;

const parser = new MarkdownParser();

// Check if file argument provided
const inputFile = process.argv[2];

let markdown = sampleMarkdown;
let filename = 'sample.md';

if (inputFile) {
  try {
    markdown = fs.readFileSync(inputFile, 'utf8');
    filename = path.basename(inputFile);
    console.log(`${c.green}✓${c.reset} Loaded ${filename}\n`);
  } catch (e) {
    console.log(`${c.yellow}Could not load ${inputFile}, using sample${c.reset}\n`);
  }
}

// Parse markdown
const result = parser.parse(markdown);

// Show frontmatter
if (result.frontmatter) {
  console.log(`${c.bold}Frontmatter:${c.reset}`);
  console.log(`${c.dim}${JSON.stringify(result.frontmatter, null, 2)}${c.reset}\n`);
}

// Show table of contents
if (result.toc.length > 0) {
  console.log(`${c.bold}Table of Contents:${c.reset}`);
  for (const item of result.toc) {
    const indent = '  '.repeat(item.level - 1);
    console.log(`${indent}${c.blue}${item.level}.${c.reset} ${item.text}`);
  }
  console.log();
}

// Show HTML output
console.log(`${c.bold}Generated HTML:${c.reset}`);
console.log(`${c.dim}${'─'.repeat(50)}${c.reset}`);
console.log(result.html.slice(0, 1500));
if (result.html.length > 1500) {
  console.log(`${c.dim}... (${result.html.length - 1500} more characters)${c.reset}`);
}
console.log(`${c.dim}${'─'.repeat(50)}${c.reset}`);

// Stats
console.log(`\n${c.bold}Stats:${c.reset}`);
console.log(`  Input:  ${markdown.length} characters`);
console.log(`  Output: ${result.html.length} characters`);
console.log(`  Headers: ${result.toc.length}`);

// Save output
const outputPath = path.join(path.dirname(process.argv[1] || __filename), 'output.html');
const fullHtml = `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>${result.frontmatter?.title || 'Document'}</title>
  <style>
    body { font-family: -apple-system, sans-serif; max-width: 800px; margin: 2rem auto; padding: 0 1rem; }
    pre { background: #f5f5f5; padding: 1rem; overflow-x: auto; }
    code { background: #f0f0f0; padding: 0.2em 0.4em; }
    pre code { background: none; padding: 0; }
    blockquote { border-left: 4px solid #ddd; margin: 0; padding-left: 1rem; color: #666; }
    table { border-collapse: collapse; width: 100%; }
    th, td { border: 1px solid #ddd; padding: 0.5rem; text-align: left; }
    th { background: #f5f5f5; }
    img { max-width: 100%; }
    hr { border: none; border-top: 1px solid #ddd; margin: 2rem 0; }
  </style>
</head>
<body>
${parser.generateToc()}
${result.html}
</body>
</html>`;

fs.writeFileSync(outputPath, fullHtml);
console.log(`\n${c.green}✓${c.reset} Saved HTML to ${outputPath}`);
console.log(`${c.green}${c.bold}Markdown processing completed!${c.reset}\n`);
