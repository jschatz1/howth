---
title: Getting Started with Howth
date: 2024-01-15
author: Alice
---

Learn how to get started with Howth, the fast JavaScript runtime.

## Installation

First, build Howth from source:

```bash
cargo build --features native-runtime -p fastnode-cli
```

## Running Your First Script

Create a file called `hello.js`:

```javascript
console.log('Hello, Howth!');
```

Then run it:

```bash
howth run --native hello.js
```

That's it! You're now running JavaScript with Howth.
