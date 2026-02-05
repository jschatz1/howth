const greet = require("./commands/greet");
const files = require("./commands/files");

// Simple argument parsing
const args = process.argv.slice(2);
const command = args[0];

// Parse flags
function parseFlags(args) {
  const flags = {};
  for (let i = 0; i < args.length; i++) {
    if (args[i].startsWith("--")) {
      const key = args[i].slice(2);
      const value = args[i + 1] && !args[i + 1].startsWith("--") ? args[++i] : true;
      flags[key] = value;
    }
  }
  return flags;
}

const flags = parseFlags(args);

// ANSI colors
const colors = {
  reset: "\x1b[0m",
  bold: "\x1b[1m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  cyan: "\x1b[36m",
};

function showHelp() {
  console.log(`
${colors.bold}Howth CLI Example${colors.reset}

${colors.cyan}Usage:${colors.reset}
  howth run cli.js <command> [options]

${colors.cyan}Commands:${colors.reset}
  greet        Say hello to someone
  files        List files in a directory
  help         Show this help message

${colors.cyan}Examples:${colors.reset}
  howth run cli.js greet --name Alice
  howth run cli.js files ./
  howth run cli.js files --ext .js ./src
`);
}

// Route to command
switch (command) {
  case "greet":
    greet(flags);
    break;
  case "files":
    const dir = args.find(a => !a.startsWith("--") && a !== "files") || ".";
    files(dir, flags);
    break;
  case "help":
  case "--help":
  case "-h":
  case undefined:
    showHelp();
    break;
  default:
    console.error(`Unknown command: ${command}`);
    showHelp();
    process.exit(1);
}
