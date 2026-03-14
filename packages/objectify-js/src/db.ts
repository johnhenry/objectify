import Database from 'better-sqlite3';
import type { ObjectRow } from './types.js';

const SCHEMA_SQL = `
CREATE TABLE IF NOT EXISTS objects (
    id          TEXT PRIMARY KEY,
    class       TEXT,
    description TEXT,
    schema      TEXT,
    created_at  TEXT NOT NULL,
    expires_at  TEXT
);
CREATE TABLE IF NOT EXISTS events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    object_id   TEXT NOT NULL REFERENCES objects(id) ON DELETE CASCADE,
    version     INTEGER NOT NULL,
    method      TEXT NOT NULL,
    state       TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    UNIQUE(object_id, version)
);
CREATE INDEX IF NOT EXISTS idx_events_object_version ON events(object_id, version);
CREATE INDEX IF NOT EXISTS idx_objects_class ON objects(class);
CREATE INDEX IF NOT EXISTS idx_objects_expires ON objects(expires_at)
    WHERE expires_at IS NOT NULL;
`;

export function openDb(dbPath: string): Database.Database {
  const db = new Database(dbPath);
  db.pragma('journal_mode = WAL');
  db.pragma('foreign_keys = ON');
  db.exec(SCHEMA_SQL);
  return db;
}

export function resolveId(db: Database.Database, prefix: string): string {
  const lower = prefix.toLowerCase();
  const rows = db
    .prepare("SELECT id FROM objects WHERE id LIKE ? || '%'")
    .all(lower) as { id: string }[];

  if (rows.length === 0) {
    throw new Error(`object not found: ${prefix}`);
  }
  if (rows.length > 1) {
    const shorts = rows.map((r) => displayId(db, r.id));
    throw new Error(
      `ambiguous id prefix '${prefix}': matches ${shorts.join(', ')}`,
    );
  }
  return rows[0].id;
}

export function displayId(db: Database.Database, fullId: string): string {
  const lower = fullId.toLowerCase();
  for (let len = 4; len <= lower.length; len++) {
    const prefix = lower.slice(0, len);
    const row = db
      .prepare("SELECT COUNT(*) as count FROM objects WHERE id LIKE ? || '%'")
      .get(prefix) as { count: number };
    if (row.count === 1) {
      return prefix;
    }
  }
  return lower;
}

export function getObject(db: Database.Database, fullId: string): ObjectRow {
  const row = db
    .prepare(
      'SELECT id, class, description, schema, created_at, expires_at FROM objects WHERE id = ?',
    )
    .get(fullId) as
    | {
        id: string;
        class: string | null;
        description: string | null;
        schema: string | null;
        created_at: string;
        expires_at: string | null;
      }
    | undefined;

  if (!row) throw new Error(`object not found: ${fullId}`);

  return {
    id: row.id,
    class: row.class,
    description: row.description,
    schema: row.schema,
    createdAt: row.created_at,
    expiresAt: row.expires_at,
  };
}

export function getStateAt(
  db: Database.Database,
  objectId: string,
  version?: number,
): unknown {
  let row: { state: string } | undefined;

  if (version !== undefined) {
    row = db
      .prepare(
        'SELECT state FROM events WHERE object_id = ? AND version = ?',
      )
      .get(objectId, version) as { state: string } | undefined;
    if (!row) throw new Error(`version ${version} not found`);
  } else {
    row = db
      .prepare(
        'SELECT state FROM events WHERE object_id = ? ORDER BY version DESC LIMIT 1',
      )
      .get(objectId) as { state: string } | undefined;
    if (!row) throw new Error('no state found for object');
  }

  return JSON.parse(row.state);
}

export function nextVersion(db: Database.Database, objectId: string): number {
  const row = db
    .prepare('SELECT MAX(version) as max FROM events WHERE object_id = ?')
    .get(objectId) as { max: number | null };
  return (row.max ?? 0) + 1;
}

export function writeEvent(
  db: Database.Database,
  objectId: string,
  method: string,
  state: unknown,
): number {
  const version = nextVersion(db, objectId);
  const stateStr = JSON.stringify(state);
  const now = new Date().toISOString();
  db.prepare(
    'INSERT INTO events (object_id, version, method, state, created_at) VALUES (?, ?, ?, ?, ?)',
  ).run(objectId, version, method, stateStr, now);
  return version;
}

export function versionCount(
  db: Database.Database,
  objectId: string,
): number {
  const row = db
    .prepare('SELECT COUNT(*) as count FROM events WHERE object_id = ?')
    .get(objectId) as { count: number };
  return row.count;
}

export function isExpired(obj: ObjectRow): boolean {
  if (!obj.expiresAt) return false;
  return new Date() > new Date(obj.expiresAt);
}

export function parseExpiryDuration(s: string): string {
  if (!s) throw new Error('empty expiry string');

  const unit = s.at(-1)!;
  const amount = parseInt(s.slice(0, -1), 10);
  if (isNaN(amount)) throw new Error(`invalid expiry: ${s}`);

  const multipliers: Record<string, number> = {
    s: 1000,
    m: 60_000,
    h: 3_600_000,
    d: 86_400_000,
    w: 604_800_000,
  };

  const ms = multipliers[unit];
  if (!ms) throw new Error(`invalid expiry unit '${unit}'. Use: s, m, h, d, w`);

  return new Date(Date.now() + amount * ms).toISOString();
}
