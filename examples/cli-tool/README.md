# CLI Tool Example

A command-line tool demonstrating Howth's Node.js API compatibility.

## Quick Start

```bash
howth run cli.js --help
howth run cli.js greet --name Alice
howth run cli.js files ./
```

## Files

- `cli.js` - Main CLI with argument parsing
- `commands/greet.js` - Greeting command
- `commands/files.js` - File listing command

## Features Demonstrated

- process.argv parsing
- File system operations (fs module)
- Path manipulation (path module)
- Console output with colors (ANSI codes)
- Module organization
