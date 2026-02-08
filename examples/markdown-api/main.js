/**
 * Markdown API Example
 *
 * Demonstrates Howth.markdown - a built-in CommonMark/GFM Markdown parser.
 * Similar to Bun.markdown.
 *
 * Run: howth run --native examples/markdown-api/main.js
 */

console.log('=== Markdown API Example ===\n');

// ============================================================================
// Basic Usage
// ============================================================================

console.log('1. Basic Markdown to HTML:');
console.log('─'.repeat(50));

const basicMd = `# Hello World

This is **bold** and *italic* text.

A paragraph with a [link](https://example.com).
`;

console.log('Input:');
console.log(basicMd);
console.log('Output:');
console.log(Howth.markdown(basicMd));

// ============================================================================
// GFM Extensions (enabled by default)
// ============================================================================

console.log('\n2. GitHub Flavored Markdown (GFM):');
console.log('─'.repeat(50));

const gfmMd = `## GFM Features

### Tables

| Feature | Supported |
|---------|-----------|
| Tables | ✓ |
| Strikethrough | ✓ |
| Task Lists | ✓ |

### Strikethrough

~~This text is struck through~~

### Task Lists

- [x] Completed task
- [ ] Pending task
- [ ] Another pending task
`;

console.log('Input: (tables, strikethrough, task lists)');
console.log(gfmMd.split('\n').slice(0, 10).join('\n') + '\n...');
console.log('\nOutput:');
console.log(Howth.markdown(gfmMd));

// ============================================================================
// Heading IDs
// ============================================================================

console.log('\n3. Heading IDs (for anchor links):');
console.log('─'.repeat(50));

const headingMd = `# Welcome to the Docs

## Getting Started

## API Reference

## FAQ
`;

console.log('Without headingIds:');
console.log(Howth.markdown(headingMd));

console.log('With headingIds: true:');
console.log(Howth.markdown(headingMd, { headingIds: true }));

// ============================================================================
// Smart Punctuation
// ============================================================================

console.log('\n4. Smart Punctuation:');
console.log('─'.repeat(50));

const punctuationMd = `"Hello," she said. "It's a beautiful day..."

He replied, "I couldn't agree more -- it's perfect."
`;

console.log('Without smart punctuation:');
console.log(Howth.markdown(punctuationMd, { smartPunctuation: false }));

console.log('With smart punctuation:');
console.log(Howth.markdown(punctuationMd, { smartPunctuation: true }));

// ============================================================================
// Code Blocks
// ============================================================================

console.log('\n5. Code Blocks:');
console.log('─'.repeat(50));

const codeMd = `Inline \`code\` and blocks:

\`\`\`javascript
function hello() {
  console.log("Hello, World!");
}
\`\`\`

\`\`\`rust
fn main() {
    println!("Hello from Rust!");
}
\`\`\`
`;

console.log(Howth.markdown(codeMd));

// ============================================================================
// Using .html() method
// ============================================================================

console.log('\n6. Alternative .html() method:');
console.log('─'.repeat(50));

console.log('Howth.markdown.html("## Heading") →');
console.log(Howth.markdown.html("## Heading"));

console.log('Howth.markdown.html("## Heading", { headingIds: true }) →');
console.log(Howth.markdown.html("## Heading", { headingIds: true }));

// ============================================================================
// Performance
// ============================================================================

console.log('\n7. Performance Test:');
console.log('─'.repeat(50));

const largeMd = `# Document Title

${Array(100).fill(`
## Section

This is a paragraph with **bold** and *italic* text.

- List item 1
- List item 2
- List item 3

| Col1 | Col2 | Col3 |
|------|------|------|
| A    | B    | C    |

\`\`\`javascript
const x = 1;
\`\`\`

`).join('\n')}
`;

const iterations = 100;
const start = Date.now();

for (let i = 0; i < iterations; i++) {
  Howth.markdown(largeMd);
}

const elapsed = Date.now() - start;
const mdSize = largeMd.length;

console.log(`Markdown size: ${(mdSize / 1024).toFixed(1)} KB`);
console.log(`Iterations: ${iterations}`);
console.log(`Total time: ${elapsed}ms`);
console.log(`Per iteration: ${(elapsed / iterations).toFixed(2)}ms`);
console.log(`Throughput: ${((mdSize * iterations) / (elapsed / 1000) / 1024 / 1024).toFixed(1)} MB/s`);

// ============================================================================
// Summary
// ============================================================================

console.log('\n=== API Summary ===\n');
console.log('Howth.markdown(content, options?)');
console.log('Howth.markdown.html(content, options?)');
console.log('\nOptions:');
console.log('  - headingIds: boolean (default: false) - Add IDs to headings');
console.log('  - gfm: boolean (default: true) - Enable GitHub Flavored Markdown');
console.log('  - smartPunctuation: boolean (default: false) - Smart quotes/dashes');

console.log('\n✓ Markdown API example complete!');
