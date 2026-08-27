import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import Database from 'better-sqlite3';
import { Objectify } from './objectify.js';
import { openDb } from './db.js';

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

// ── schema compatibility ───────────────────────────────────────────────────

function withTmpDb(fn: (dbPath: string) => void): void {
  const dir = mkdtempSync(join(tmpdir(), 'objectify-js-schema-test-'));
  const dbPath = join(dir, 'objectify.db');
  try {
    fn(dbPath);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('openDb accepts a fresh (nonexistent) database file', () => {
  withTmpDb((dbPath) => {
    const db = openDb(dbPath);
    assert.doesNotThrow(() => db.prepare('SELECT COUNT(*) FROM objects').get());
    db.close();
  });
});

test('openDb accepts a pre-existing, correctly-shaped database (e.g. from the Rust CLI, which never sets user_version)', () => {
  withTmpDb((dbPath) => {
    // Simulate a store created by the Rust CLI: correct tables/columns, but
    // user_version left at its SQLite default of 0.
    const seed = new Database(dbPath);
    seed.exec(`
      CREATE TABLE objects (
        id TEXT PRIMARY KEY, class TEXT, description TEXT, schema TEXT,
        created_at TEXT NOT NULL, expires_at TEXT
      );
      CREATE TABLE events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        object_id TEXT NOT NULL REFERENCES objects(id) ON DELETE CASCADE,
        version INTEGER NOT NULL, method TEXT NOT NULL, state TEXT NOT NULL,
        created_at TEXT NOT NULL, UNIQUE(object_id, version)
      );
    `);
    seed.close();

    assert.doesNotThrow(() => {
      const db = openDb(dbPath);
      db.close();
    });
  });
});

test('openDb rejects a SQLite file with an unrelated schema (same table name, different shape)', () => {
  withTmpDb((dbPath) => {
    const seed = new Database(dbPath);
    // A table named "objects" that has nothing to do with objectify's schema.
    seed.exec('CREATE TABLE objects (name TEXT, price REAL);');
    seed.close();

    assert.throws(() => openDb(dbPath), /incompatible objectify database/);
  });
});

test('openDb rejects a database with a mismatched schema version', () => {
  withTmpDb((dbPath) => {
    const seed = new Database(dbPath);
    seed.exec(`
      CREATE TABLE objects (
        id TEXT PRIMARY KEY, class TEXT, description TEXT, schema TEXT,
        created_at TEXT NOT NULL, expires_at TEXT
      );
      CREATE TABLE events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        object_id TEXT NOT NULL REFERENCES objects(id) ON DELETE CASCADE,
        version INTEGER NOT NULL, method TEXT NOT NULL, state TEXT NOT NULL,
        created_at TEXT NOT NULL, UNIQUE(object_id, version)
      );
    `);
    // A future/foreign schema version this code doesn't know how to handle.
    seed.pragma('user_version = 99');
    seed.close();

    assert.throws(() => openDb(dbPath), /incompatible objectify database/);
  });
});
