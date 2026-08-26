#!/usr/bin/env node
'use strict';

/**
 * Platform-detection shim for the `objectify` CLI.
 *
 * The real compiled binary lives in one of five tiny per-platform npm
 * packages (each published from CI, one per Rust build target) and is
 * installed via `optionalDependencies` — npm only installs the ones whose
 * `os`/`cpu` fields match the current machine, so exactly one (usually)
 * ends up on disk. This script finds it, verifies its checksum against the
 * value pinned in this package's checksums.json (see verifyChecksum below),
 * and execs it, passing through argv/stdio/exit code unchanged.
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
const { createHash } = require('crypto');
const fs = require('fs');
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

// Resolves to { binaryPath, platformName } where platformName is the
// hyphenated platform identifier (e.g. "darwin-arm64") used as the key into
// checksums.json — or platformName: null when OBJECTIFY_BINARY_PATH is used,
// since a locally-built binary has no pinned checksum to verify against.
function resolveBinaryPath() {
  if (OBJECTIFY_BINARY_PATH) {
    return { binaryPath: OBJECTIFY_BINARY_PATH, platformName: null };
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

  const platformName = pkg.replace('@johnhenry/objectify-', '');

  try {
    const binaryPath = require.resolve(`${pkg}/${binSubpath(platform)}`);
    return { binaryPath, platformName };
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

// checksums.json (published as part of this package, see .github/workflows/
// release.yml) pins the expected sha256 of each platform's binary at the
// version this package was published with. Verifying it here means a
// compromised npm token, CI runner, or registry mirror can't silently swap
// in a different binary for one of the five platform packages without also
// tripping this check (short of also compromising the main package's
// checksums.json in the same way, which `npm publish --provenance` in the
// release workflow independently guards against).
function loadChecksums() {
  const checksumsPath = path.join(__dirname, '..', 'checksums.json');
  try {
    return JSON.parse(fs.readFileSync(checksumsPath, 'utf8'));
  } catch (e) {
    if (e && e.code === 'ENOENT') {
      // Published before checksums.json existed, or a non-npm install layout.
      // Fail open rather than breaking installs — this is a defense-in-depth
      // check, not the primary integrity mechanism (that's --provenance).
      return null;
    }
    fail(`Failed to read checksums.json: ${e && e.message}`);
  }
}

function sha256File(filePath) {
  const hash = createHash('sha256');
  hash.update(fs.readFileSync(filePath));
  return hash.digest('hex');
}

function verifyChecksum(binaryPath, platformName) {
  if (platformName === null) {
    return; // OBJECTIFY_BINARY_PATH escape hatch — nothing pinned to check.
  }

  const checksums = loadChecksums();
  if (!checksums) {
    return;
  }

  const expected = checksums[platformName];
  if (!expected) {
    // checksums.json exists but doesn't cover this platform — don't block
    // execution over a manifest gap, but don't pretend we verified it either.
    return;
  }

  const actual = sha256File(binaryPath);
  if (actual !== expected) {
    fail(
      `Checksum mismatch for the objectify binary (${platformName}).\n\n` +
        `  expected: ${expected}\n` +
        `  actual:   ${actual}\n\n` +
        `This means the installed binary does not match the one published by\n` +
        `objectify's release CI for this version. It may have been tampered\n` +
        `with, corrupted, or come from an untrusted registry mirror. Refusing\n` +
        `to execute it.\n\n` +
        `Try reinstalling: npm install --include=optional @johnhenry/objectify\n` +
        `If the mismatch persists, please open an issue:\n` +
        `  https://github.com/johnhenry/objectify/issues`,
    );
  }
}

function fail(message) {
  console.error(`[objectify] ${message}`);
  process.exit(1);
}

function main() {
  const { binaryPath, platformName } = resolveBinaryPath();
  verifyChecksum(binaryPath, platformName);

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
