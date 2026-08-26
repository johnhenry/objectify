#!/usr/bin/env node
// An object written from JavaScript is immediately visible to the Rust CLI —
// and vice versa. Both talk to the same SQLite file inside .objectify/, so a
// Node process and the `objectify` binary are two clients of one store.
//
// Requires: packages/objectify-js built (see examples/README.md — this needs
// the better-sqlite3 ^13 bump from PR #3 on current Node).
import { mkdtempSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { Objectify } from '../packages/objectify-js/dist/index.js';

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const releaseBin = join(repoRoot, 'target', 'release', 'objectify');

// Same convention as the shell examples: prefer the release binary, fall back
// to `cargo run --release --quiet --`.
function cli(args, opts = {}) {
  const out = existsSync(releaseBin)
    ? execFileSync(releaseBin, args, opts)
    : execFileSync(
        'cargo',
        ['run', '--manifest-path', join(repoRoot, 'Cargo.toml'), '--release', '--quiet', '--', ...args],
        opts,
      );
  return out.toString().trim();
}

const tmp = mkdtempSync(join(tmpdir(), 'objectify-example-'));
try {
  const dir = join(tmp, '.objectify');
  mkdirSync(join(dir, 'classes'), { recursive: true });

  // --- JavaScript writes... ---
  const store = new Objectify({ dir });
  const id = store.create({ description: 'written from JS' });
  store.use(id).set({ source: 'javascript', count: 1 });
  console.log(`JS created object ${id} and wrote state`);

  // --- ...the CLI reads it back (a completely separate process) ---
  const cliView = JSON.parse(cli(['use', id, 'get'], { cwd: tmp }));
  console.log('CLI sees:', cliView);
  assert.deepEqual(cliView, { source: 'javascript', count: 1 });

  // --- The CLI writes... ---
  cli(['use', id, 'set', '{"source": "cli", "count": 2}'], { cwd: tmp });

  // --- ...and JS sees the new version, with full shared history ---
  const ref = store.use(id);
  console.log('JS sees:', ref.get());
  assert.deepEqual(ref.get(), { source: 'cli', count: 2 });

  const log = ref.log();
  console.log(`history has ${log.length} versions: ${log.map((e) => e.method).join(' -> ')}`);
  assert.equal(log.length, 3); // create (JS), set (JS), set (CLI)

  store.close();
  console.log('\nOK: JS adapter and CLI operated on the same object interchangeably');
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
