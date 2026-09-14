#!/usr/bin/env node
// The full versioned lifecycle — write, time-travel, diff, rewind, fork, gc —
// is available in-process from JavaScript, no CLI or subprocess involved.
//
// Requires: packages/objectify-js built (see examples/README.md — this needs
// the better-sqlite3 ^13 bump from PR #3 on current Node).
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { Objectify } from '../packages/objectify-js/dist/index.js';

const tmp = mkdtempSync(join(tmpdir(), 'objectify-example-'));
try {
  const dir = join(tmp, '.objectify');
  mkdirSync(join(dir, 'classes'), { recursive: true });
  const store = new Objectify({ dir });

  // Every write is a version.
  const id = store.create({ description: 'feature flags' }); // v1: create
  const ref = store.use(id);
  ref.set({ darkMode: false });                              // v2
  ref.set({ darkMode: true, beta: true });                   // v3

  // Time travel: any past version stays readable.
  assert.deepEqual(ref.get(2), { darkMode: false });
  console.log('v2 state:', ref.get(2));

  // Diff between versions is an RFC 6902 JSON Patch.
  const patch = ref.diff(2, 3);
  console.log('v2 -> v3 patch:', patch);
  assert.ok(patch.some((op) => op.op === 'replace' && op.path === '/darkMode'));
  assert.ok(patch.some((op) => op.op === 'add' && op.path === '/beta'));

  // Rewind restores old state as a NEW version — history is never deleted.
  const { rewoundTo, newVersion } = ref.rewind(2);
  console.log(`rewound to v${rewoundTo}, written as v${newVersion}`);
  assert.deepEqual(ref.get(), { darkMode: false });
  assert.equal(ref.log().length, 4);

  // Forks copy state into an independent object.
  const forkId = ref.fork();
  store.use(forkId).set({ darkMode: true, forked: true });
  assert.deepEqual(ref.get(), { darkMode: false }); // original untouched
  console.log(`fork ${forkId} diverged; original still:`, ref.get());

  // Expiry + gc: expired objects are reaped, others survive.
  const ephemeral = store.create({ description: 'scratch', expire: '1s' });
  await new Promise((r) => setTimeout(r, 1500));
  const deleted = store.gc();
  console.log(`gc deleted ${deleted} expired object(s)`);
  assert.equal(deleted, 1);
  assert.throws(() => store.use(ephemeral));
  assert.equal(ref.log().length, 4); // survivor untouched

  store.close();
  console.log('\nOK: versioning, diff, rewind, fork, and gc all work from JS');
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
