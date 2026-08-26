#!/usr/bin/env node
'use strict';

/**
 * Platform-detection shim for the `objectify` CLI.
 *
 * The real compiled binary lives in one of five tiny per-platform npm
 * packages (each published from CI, one per Rust build target) and is
 * installed via `optionalDependencies` — npm only installs the ones whose
 * `os`/`cpu` fields match the current machine, so exactly one (usually)
 * ends up on disk. This script finds it and execs it, passing through
 * argv/stdio/exit code unchanged.
 *
 * Deliberately does NOT do what esbuild's install.js / turbo's bin/turbo do:
 * no postinstall network download, no automatic "npm install" repair
 * attempt if the optional dependency didn't land. If the right platform
 * package is missing, this prints a clear, actionable error instead of
 * silently reaching out to the network. See objectify's npm/README (or
 * LESSONS.md in the adopt-library skill) for why that's a first-pass
 * simplification, not a permanent design decision.
 */

const { spawnSync } = require('child_process');
const path = require('path');

// Escape hatch for local development / testing against a freshly-built
// binary without publishing a platform package first.
const OBJECTIFY_BINARY_PATH = process.env.OBJECTIFY_BINARY_PATH;

const PACKAGES = {
  'darwin arm64': '@johnhenry/objectify-darwin-arm64',
  'darwin x64': '@johnhenry/objectify-darwin-x64',
  'linux arm64': '@johnhenry/objectify-linux-arm64',
  'linux x64': '@johnhenry/objectify-linux-x64',
  'win32 x64': '@johnhenry/objectify-win32-x64',
};

function binSubpath(platform) {
  return platform === 'win32' ? 'bin/objectify.exe' : 'bin/objectify';
}

function resolveBinaryPath() {
  if (OBJECTIFY_BINARY_PATH) {
    return OBJECTIFY_BINARY_PATH;
  }

  const platform = process.platform;
  const arch = process.arch;
  const key = `${platform} ${arch}`;
  const pkg = PACKAGES[key];

  if (!pkg) {
    fail(
      `Unsupported platform: ${key}\n\n` +
        `objectify currently ships prebuilt binaries for:\n` +
        Object.keys(PACKAGES)
          .map((k) => `  - ${k}`)
          .join('\n') +
        `\n\nIf you're on a platform not listed here, please open an issue:\n` +
        `  https://github.com/johnhenry/objectify/issues`,
    );
  }

  try {
    return require.resolve(`${pkg}/${binSubpath(platform)}`);
  } catch (e) {
    fail(
      `Could not find the objectify binary for your platform (${key}).\n\n` +
        `Expected it in the optional dependency "${pkg}", which npm should\n` +
        `have installed automatically alongside "@johnhenry/objectify".\n\n` +
        `This usually means the optional dependency failed to install or was\n` +
        `skipped. Try:\n\n` +
        `  npm install --include=optional @johnhenry/objectify\n\n` +
        `If you're using a package manager other than npm, make sure it\n` +
        `doesn't skip optional dependencies (some configurations of pnpm/\n` +
        `Yarn need "supportedArchitectures" set explicitly).\n\n` +
        `Underlying error: ${e && e.message}`,
    );
  }
}

function fail(message) {
  console.error(`[objectify] ${message}`);
  process.exit(1);
}

function main() {
  const binaryPath = resolveBinaryPath();
  const result = spawnSync(binaryPath, process.argv.slice(2), {
    stdio: 'inherit',
  });

  if (result.error) {
    fail(`Failed to run "${path.basename(binaryPath)}": ${result.error.message}`);
  }

  if (result.signal) {
    // Re-raise the same signal so the parent shell sees the expected
    // "killed by signal" behavior rather than a plain exit code.
    process.kill(process.pid, result.signal);
    return;
  }

  process.exit(result.status === null ? 1 : result.status);
}

main();
