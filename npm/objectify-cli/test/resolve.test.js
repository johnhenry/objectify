'use strict';

// Smoke test: the shim resolves the right platform package per platform,
// including musl vs glibc on Linux, and every package it can name has a
// matching manifest in this repo that is pinned in optionalDependencies.
// Run with: node --test npm/objectify-cli/test

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { PACKAGES, platformPackage } = require('../bin/objectify.js');

const NPM_DIR = path.join(__dirname, '..', '..');
const P = '@johnhenry/objectify-cli-';

test('resolves the right package per platform', () => {
  const cases = [
    ['darwin', 'arm64', false, `${P}darwin-arm64`],
    ['darwin', 'x64', false, `${P}darwin-x64`],
    ['linux', 'x64', false, `${P}linux-x64`],
    ['linux', 'x64', true, `${P}linux-x64-musl`],
    ['linux', 'arm64', false, `${P}linux-arm64`],
    ['win32', 'x64', false, `${P}win32-x64`],
  ];
  for (const [platform, arch, musl, expected] of cases) {
    assert.equal(platformPackage(platform, arch, musl), expected, `${platform} ${arch} musl=${musl}`);
  }
});

test('unsupported combinations resolve to nothing', () => {
  assert.equal(platformPackage('linux', 'arm64', true), undefined); // no arm64 musl build yet
  assert.equal(platformPackage('freebsd', 'x64', false), undefined);

});

test('every mapped package has a manifest and an exact optionalDependencies pin', () => {
  const shim = JSON.parse(fs.readFileSync(path.join(NPM_DIR, 'objectify-cli', 'package.json'), 'utf8'));
  for (const pkg of Object.values(PACKAGES)) {
    const dir = pkg.slice(P.length);
    const manifest = JSON.parse(fs.readFileSync(path.join(NPM_DIR, dir, 'package.json'), 'utf8'));
    assert.equal(manifest.name, pkg);
    assert.equal(shim.optionalDependencies[pkg], manifest.version, `${pkg} pin`);
    assert.equal(manifest.version, shim.version, `${pkg} version matches the shim`);
  }
  assert.deepEqual(
    Object.keys(shim.optionalDependencies).sort(),
    Object.values(PACKAGES).sort(),
  );
});

test('linux packages declare libc so npm installs only the matching one', () => {
  const libc = (d) => JSON.parse(fs.readFileSync(path.join(NPM_DIR, d, 'package.json'), 'utf8')).libc;
  assert.deepEqual(libc('linux-x64'), ['glibc']);
  assert.deepEqual(libc('linux-arm64'), ['glibc']);
  assert.deepEqual(libc('linux-x64-musl'), ['musl']);
});
