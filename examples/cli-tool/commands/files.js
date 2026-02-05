const fs = require("fs");
const path = require("path");

const colors = {
  reset: "\x1b[0m",
  bold: "\x1b[1m",
  blue: "\x1b[34m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
};

module.exports = function files(dir, flags) {
  const absolutePath = path.resolve(dir);

  if (!fs.existsSync(absolutePath)) {
    console.error("Directory not found: " + absolutePath);
    process.exit(1);
  }

  const entries = fs.readdirSync(absolutePath, { withFileTypes: true });
  const ext = flags.ext;

  console.log(colors.bold + "Contents of " + absolutePath + colors.reset + "\n");

  let fileCount = 0;
  let dirCount = 0;

  for (const entry of entries) {
    if (!flags.all && entry.name.startsWith(".")) continue;
    if (ext && !entry.isDirectory() && !entry.name.endsWith(ext)) continue;

    if (entry.isDirectory()) {
      console.log(colors.blue + entry.name + "/" + colors.reset);
      dirCount++;
    } else {
      const stats = fs.statSync(path.join(absolutePath, entry.name));
      const size = formatSize(stats.size);
      console.log(colors.green + entry.name + colors.reset + " " + colors.yellow + "(" + size + ")" + colors.reset);
      fileCount++;
    }
  }

  console.log("\n" + fileCount + " file(s), " + dirCount + " directory(ies)");
};

function formatSize(bytes) {
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}
