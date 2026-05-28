#!/usr/bin/env node
// Postinstall script — downloads the correct binary for the current platform

const https = require("https");
const fs = require("fs");
const path = require("path");
const zlib = require("zlib");

const VERSION = require("./package.json").version;
const REPO = "nousresearch/mimo-tui-rs";

const PLATFORM_MAP = {
  "darwin": "apple-darwin",
  "linux": "unknown-linux-gnu",
  "win32": "pc-windows-msvc",
};

const ARCH_MAP = {
  "x64": "x86_64",
  "arm64": "aarch64",
};

function getTarget() {
  const platform = PLATFORM_MAP[process.platform];
  const arch = ARCH_MAP[process.arch];

  if (!platform || !arch) {
    console.error(
      `Unsupported platform: ${process.platform} ${process.arch}\n` +
      `Supported: macOS (x64/arm64), Linux (x64/arm64), Windows (x64)\n` +
      `Install from source: cargo install mimo-tui`
    );
    process.exit(1);
  }

  return `${arch}-${platform}`;
}

function getBinName() {
  return process.platform === "win32" ? "mimo-tui.exe" : "mimo-tui";
}

function getDownloadUrl(target) {
  // GitHub release URL pattern
  return `https://github.com/${REPO}/releases/download/v${VERSION}/mimo-tui-${target}.tar.gz`;
}

function download(url) {
  return new Promise((resolve, reject) => {
    const follow = (u, redirects = 0) => {
      if (redirects > 5) return reject(new Error("Too many redirects"));
      https.get(u, { headers: { "User-Agent": "mimo-tui-npm" } }, (res) => {
        if ([301, 302, 307].includes(res.statusCode)) {
          return follow(res.headers.location, redirects + 1);
        }
        if (res.statusCode !== 200) {
          return reject(new Error(`HTTP ${res.statusCode}: ${url}`));
        }
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      }).on("error", reject);
    };
    follow(url);
  });
}

async function extract(buffer, destPath) {
  const decompressed = zlib.gunzipSync(buffer);
  // tar extraction — find the binary
  // Simple tar parser for the binary file
  let offset = 0;
  while (offset < decompressed.length) {
    // tar header is 512 bytes
    if (offset + 512 > decompressed.length) break;

    const name = decompressed.toString("utf8", offset, offset + 100).replace(/\0/g, "");
    const sizeOctal = decompressed.toString("utf8", offset + 124, offset + 136).trim();
    const size = parseInt(sizeOctal, 8) || 0;

    if (!name) break;

    const dataStart = offset + 512;
    const dataEnd = dataStart + Math.ceil(size / 512) * 512;

    const binName = getBinName();
    if (name.endsWith(binName) || name === binName) {
      fs.writeFileSync(destPath, decompressed.slice(dataStart, dataStart + size));
      fs.chmodSync(destPath, 0o755);
      return true;
    }

    offset = dataEnd;
  }
  return false;
}

async function main() {
  const target = getTarget();
  const url = getDownloadUrl(target);
  const binDir = path.join(__dirname, "bin");
  const binPath = path.join(binDir, getBinName());

  // Skip if already installed (e.g., dev mode)
  if (fs.existsSync(binPath)) {
    console.log(`mimo-tui: binary already exists at ${binPath}`);
    return;
  }

  console.log(`mimo-tui: downloading for ${target}...`);
  console.log(`  ${url}`);

  try {
    fs.mkdirSync(binDir, { recursive: true });
    const buffer = await download(url);
    const extracted = await extract(buffer, binPath);

    if (extracted) {
      console.log(`mimo-tui: installed to ${binPath}`);
    } else {
      throw new Error("Binary not found in archive");
    }
  } catch (err) {
    console.error(`\nmimo-tui: download failed — ${err.message}`);
    console.error(`\nManual install options:`);
    console.error(`  cargo install mimo-tui`);
    console.error(`  Download from: https://github.com/${REPO}/releases\n`);
    // Don't fail install — npm will still succeed
    process.exit(0);
  }
}

main();
