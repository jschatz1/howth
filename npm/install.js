#!/usr/bin/env node

const { execSync, spawn } = require("child_process");
const fs = require("fs");
const path = require("path");
const https = require("https");
const zlib = require("zlib");

const REPO = "jschatz1/howth";
const VERSION = require("./package.json").version;

function getPlatformInfo() {
  const platform = process.platform;
  const arch = process.arch;

  let osPart;
  switch (platform) {
    case "darwin":
      osPart = "apple-darwin";
      break;
    case "linux":
      osPart = "unknown-linux-gnu";
      break;
    case "win32":
      osPart = "pc-windows-msvc";
      break;
    default:
      throw new Error(`Unsupported platform: ${platform}`);
  }

  let archPart;
  switch (arch) {
    case "x64":
      archPart = "x86_64";
      break;
    case "arm64":
      archPart = "aarch64";
      break;
    default:
      throw new Error(`Unsupported architecture: ${arch}`);
  }

  const ext = platform === "win32" ? "zip" : "tar.gz";
  const target = `${archPart}-${osPart}`;

  return { target, ext, platform };
}

function downloadFile(url) {
  return new Promise((resolve, reject) => {
    const request = (url) => {
      https
        .get(url, (response) => {
          if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
            request(response.headers.location);
            return;
          }

          if (response.statusCode !== 200) {
            reject(new Error(`Failed to download: ${response.statusCode}`));
            return;
          }

          const chunks = [];
          response.on("data", (chunk) => chunks.push(chunk));
          response.on("end", () => resolve(Buffer.concat(chunks)));
          response.on("error", reject);
        })
        .on("error", reject);
    };
    request(url);
  });
}

async function extractTarGz(buffer, destDir) {
  const tar = require("tar");
  const tmpFile = path.join(destDir, "archive.tar.gz");
  fs.writeFileSync(tmpFile, buffer);

  await tar.extract({
    file: tmpFile,
    cwd: destDir,
    strip: 1,
  });

  fs.unlinkSync(tmpFile);
}

async function extractTarGzBuiltin(buffer, destDir) {
  // Use system tar if available
  const tmpFile = path.join(destDir, "archive.tar.gz");
  fs.writeFileSync(tmpFile, buffer);

  try {
    execSync(`tar xzf "${tmpFile}" --strip-components=1`, { cwd: destDir, stdio: "ignore" });
    fs.unlinkSync(tmpFile);
  } catch (e) {
    fs.unlinkSync(tmpFile);
    throw e;
  }
}

async function main() {
  const { target, ext, platform } = getPlatformInfo();
  const tag = `v${VERSION}`;
  const archiveName = `howth-${tag}-${target}.${ext}`;
  const url = `https://github.com/${REPO}/releases/download/${tag}/${archiveName}`;

  console.log(`Downloading howth ${tag} for ${target}...`);

  const binDir = path.join(__dirname, "bin");
  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }

  try {
    const buffer = await downloadFile(url);
    console.log("Extracting...");

    if (ext === "tar.gz") {
      await extractTarGzBuiltin(buffer, binDir);
    } else {
      // For zip on Windows, use PowerShell
      const tmpFile = path.join(binDir, "archive.zip");
      fs.writeFileSync(tmpFile, buffer);
      execSync(`powershell -Command "Expand-Archive -Path '${tmpFile}' -DestinationPath '${binDir}' -Force"`, { stdio: "ignore" });
      fs.unlinkSync(tmpFile);
    }

    // Make binary executable on Unix
    const binaryName = platform === "win32" ? "howth.exe" : "howth";
    const binaryPath = path.join(binDir, binaryName);

    if (fs.existsSync(binaryPath) && platform !== "win32") {
      fs.chmodSync(binaryPath, 0o755);
    }

    console.log("howth installed successfully!");
  } catch (error) {
    console.error("Failed to install howth:", error.message);
    console.error("");
    console.error("You can install manually from:");
    console.error(`  ${url}`);
    console.error("");
    console.error("Or use the shell installer:");
    console.error("  curl -fsSL https://howth.run/install.sh | sh");
    process.exit(1);
  }
}

main();
