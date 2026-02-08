/**
 * Sass/SCSS Example
 *
 * Demonstrates howth's built-in Sass preprocessing via grass.
 * Shows variables, nesting, mixins, functions, and loops.
 */

const fs = require('fs');
const path = require('path');

console.log('=== Sass/SCSS Example ===\n');

// Read the SCSS file
const scssPath = path.join(__dirname, 'styles.scss');
const scssContent = fs.readFileSync(scssPath, 'utf-8');

console.log('Input SCSS file features:');
console.log('  - Variables ($primary-color, $spacing-unit, etc.)');
console.log('  - Mixins (@mixin flex-center, @mixin button-variant)');
console.log('  - Nesting (.header { .nav { .nav-link { ... } } })');
console.log('  - Functions (darken(), lighten(), map-get())');
console.log('  - Loops (@for, @each)');
console.log('  - Conditionals (@if)');
console.log('  - Maps and lists');
console.log('  - Responsive breakpoints with @media\n');

// Count features in the SCSS
const features = {
  variables: (scssContent.match(/\$[a-z-]+:/g) || []).length,
  mixins: (scssContent.match(/@mixin\s+/g) || []).length,
  includes: (scssContent.match(/@include\s+/g) || []).length,
  nestingLevels: Math.max(...scssContent.split('\n').map(line => {
    const indent = line.match(/^\s*/)[0].length;
    return Math.floor(indent / 2);
  })),
  forLoops: (scssContent.match(/@for\s+/g) || []).length,
  eachLoops: (scssContent.match(/@each\s+/g) || []).length,
};

console.log('SCSS Statistics:');
console.log(`  Variables defined: ${features.variables}`);
console.log(`  Mixins defined: ${features.mixins}`);
console.log(`  Mixin includes: ${features.includes}`);
console.log(`  Max nesting depth: ${features.nestingLevels}`);
console.log(`  @for loops: ${features.forLoops}`);
console.log(`  @each loops: ${features.eachLoops}`);
console.log(`  Total lines: ${scssContent.split('\n').length}\n`);

// Show a snippet of the SCSS
console.log('Sample SCSS (button mixin):');
console.log('─'.repeat(50));
const mixinMatch = scssContent.match(/@mixin button-variant[\s\S]*?^\}/m);
if (mixinMatch) {
  console.log(mixinMatch[0].split('\n').slice(0, 12).join('\n'));
}
console.log('─'.repeat(50));

console.log('\nTo compile this SCSS with howth dev server:');
console.log('  1. Import it in your JS: import "./styles.scss"');
console.log('  2. Or use it as CSS Module: import styles from "./styles.module.scss"');
console.log('\nThe dev server will:');
console.log('  - Compile SCSS to CSS using grass (pure Rust)');
console.log('  - Apply autoprefixer for browser compatibility');
console.log('  - Minify for production builds');
console.log('  - Support CSS Modules for scoped class names\n');

// Generate sample HTML to demonstrate the styles
const sampleHtml = `<!DOCTYPE html>
<html>
<head>
  <title>Sass Example</title>
  <link rel="stylesheet" href="styles.css">
</head>
<body>
  <header class="header">
    <nav class="nav">
      <a href="#" class="nav-link active">Home</a>
      <a href="#" class="nav-link">About</a>
      <a href="#" class="nav-link">Contact</a>
    </nav>
    <h1 class="title">Sass/SCSS Demo</h1>
  </header>

  <main class="container mt-lg">
    <div class="grid grid-cols-3">
      <div class="card">
        <div class="card-header">
          <h3>Primary Card</h3>
        </div>
        <div class="card-body">
          <p>This card demonstrates Sass nesting and mixins.</p>
        </div>
        <div class="card-footer">
          <button class="btn btn-outline">Cancel</button>
          <button class="btn btn-primary">Submit</button>
        </div>
      </div>

      <div class="card">
        <div class="card-header">
          <h3>Secondary Card</h3>
        </div>
        <div class="card-body">
          <p class="text-secondary">Using color utilities from @each loop.</p>
        </div>
        <div class="card-footer">
          <button class="btn btn-secondary">Action</button>
        </div>
      </div>

      <div class="card">
        <div class="card-header">
          <h3>Danger Card</h3>
        </div>
        <div class="card-body">
          <p class="text-danger">Danger color variant.</p>
        </div>
        <div class="card-footer">
          <button class="btn btn-danger">Delete</button>
        </div>
      </div>
    </div>
  </main>
</body>
</html>`;

fs.writeFileSync(path.join(__dirname, 'index.html'), sampleHtml);
console.log('Generated index.html to demonstrate the styles.\n');

console.log('✓ Sass example ready!');
console.log('  Run with howth dev to see live SCSS compilation.');
