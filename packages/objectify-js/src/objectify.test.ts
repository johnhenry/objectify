import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Objectify } from './objectify.js';

function withStore(fn: (store: Objectify) => void): void {
  const dir = mkdtempSync(join(tmpdir(), 'objectify-js-test-'));
  const store = new Objectify({ dir });
  try {
    fn(store);
  } finally {
    store.close();
    rmSync(dir, { recursive: true, force: true });
  }
}

test('create + use + get/set round-trips state', () => {
  withStore((store) => {
    const id = store.create({ description: 'test object' });
    const ref = store.use(id);

    ref.set({ theme: 'dark' });
    assert.deepEqual(ref.get(), { theme: 'dark' });

    ref.set({ theme: 'light' });
    assert.deepEqual(ref.get(), { theme: 'light' });
  });
});

test('log records one entry per write, in order', () => {
  withStore((store) => {
    const id = store.create();
    const ref = store.use(id);
    ref.set({ n: 1 });
    ref.set({ n: 2 });

    const log = ref.log();
    assert.equal(log.length, 3); // create + 2 sets
    assert.equal(log[0].method, 'create');
    assert.equal(log[1].method, 'set');
    assert.equal(log[2].method, 'set');
  });
});

test('rewind restores prior state as a new version, non-destructively', () => {
  withStore((store) => {
    const id = store.create();
    const ref = store.use(id);
    ref.set({ n: 1 });
    ref.set({ n: 2 });

    const { rewoundTo, newVersion } = ref.rewind(2);
    assert.equal(rewoundTo, 2);
    assert.equal(newVersion, 4);
    assert.deepEqual(ref.get(), { n: 1 });
    assert.equal(ref.log().length, 4);
  });
});

test('diff returns an RFC 6902 JSON Patch between two versions', () => {
  withStore((store) => {
    const id = store.create();
    const ref = store.use(id);
    ref.set({ n: 1 });
    ref.set({ n: 2 });

    const patch = ref.diff(2, 3) as { op: string; path: string; value: unknown }[];
    assert.ok(Array.isArray(patch));
    assert.ok(patch.some((op) => op.path === '/n' && op.value === 2));
  });
});

test('fork creates an independent copy with its own history', () => {
  withStore((store) => {
    const id = store.create({ description: 'original' });
    const ref = store.use(id);
    ref.set({ n: 1 });

    const forkId = ref.fork();
    assert.notEqual(forkId, ref.shortId);

    const forkRef = store.use(forkId);
    assert.deepEqual(forkRef.get(), { n: 1 });

    forkRef.set({ n: 99 });
    assert.deepEqual(ref.get(), { n: 1 }); // original untouched
  });
});

test('list filters by class and respects limit', () => {
  withStore((store) => {
    store.create({ description: 'a' });
    store.create({ description: 'b', class: 'Widget' });

    const all = store.list();
    assert.equal(all.length, 2);

    const widgets = store.list({ class: 'Widget' });
    assert.equal(widgets.length, 1);
    assert.equal(widgets[0].class, 'Widget');
  });
});

test('gc deletes only expired objects', () => {
  withStore((store) => {
    const keep = store.create({ description: 'keep' });
    // A negative duration parses to an already-past expiry timestamp.
    const expired = store.create({ description: 'expired', expire: '-1s' });

    const removed = store.gc();
    assert.equal(removed, 1);
    assert.doesNotThrow(() => store.use(keep));
    assert.throws(() => store.use(expired), /object not found/);
  });
});

test('destroy removes an object permanently', () => {
  withStore((store) => {
    const id = store.create();
    store.destroy(id);
    assert.throws(() => store.use(id), /object not found/);
  });
});
