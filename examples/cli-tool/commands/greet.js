const colors = {
  reset: "\x1b[0m",
  bold: "\x1b[1m",
  green: "\x1b[32m",
};

module.exports = function greet(flags) {
  const name = flags.name || "World";
  const greeting = flags.greeting || "Hello";

  console.log(colors.green + colors.bold + greeting + ", " + name + "!" + colors.reset);

  if (flags.verbose) {
    console.log("\nRun at: " + new Date().toISOString());
    console.log("Platform: " + process.platform);
    console.log("Node version: " + process.version);
  }
};
