#!/usr/bin/env node

const { existsSync } = require("fs");
const os = require("os");
const { join } = require("path");
const { spawnSync } = require("child_process");

function isMusl() {
  return (
    existsSync("/lib/ld-musl-x86_64.so.1") ||
    existsSync("/lib/ld-musl-aarch64.so.1")
  );
}

function getExePath() {
  let platform = os.platform();
  let arch = os.arch();

  if (platform === "win32" || platform === "cygwin") {
    platform = "windows";
  }

  let libc = "";
  if (platform === "linux" && isMusl()) {
    libc = "-musl";
  }

  const scope = "@visiblelightio";
  const pkgName = `${scope}/pnpm-catalog-lint-${platform}-${arch}${libc}`;
  const binName =
    platform === "windows" ? "pnpm-catalog-lint.exe" : "pnpm-catalog-lint";

  try {
    return require.resolve(`${pkgName}/${binName}`);
  } catch {
    const localBin = join(__dirname, "..", "..", "target", "release", binName);
    if (existsSync(localBin)) {
      return localBin;
    }

    throw new Error(
      `Unsupported platform: ${platform}-${arch}${libc}. ` +
        `Please open an issue at https://github.com/visiblelightio/pnpm-catalog-lint/issues`
    );
  }
}

function run() {
  const exePath = getExePath();
  const args = process.argv.slice(2);
  const result = spawnSync(exePath, args, { stdio: "inherit" });
  process.exit(result.status ?? 1);
}

run();
