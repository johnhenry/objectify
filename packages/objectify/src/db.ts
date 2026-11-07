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

// Bumped whenever the schema shape changes in a way older/newer clients can't
// safely interoperate with. NOTE: the `objectify` Rust CLI creates the same
// tables but does not (yet) set `user_version`, so a freshly-created or
// pre-existing CLI-created database has `user_version = 0` — that is treated
// as "unversioned, unknown, verify structurally" rather than an incompatible
// version, to avoid breaking every store that predates this check.
const SCHEMA_VERSION = 1;

const EXPECTED_COLUMNS: Record<'objects' | 'events', string[]> = {
  objects: ['id', 'class', 'description', 'schema', 'created_at', 'expires_at'],
  events: ['id', 'object_id', 'version', 'method', 'state', 'created_at'],
};

function tableColumns(db: Database.Database, table: string): string[] {
  const rows = db.prepare(`PRAGMA table_info(${table})`).all() as { name: string }[];
  return rows.map((r) => r.name);
}

/**
 * Guard against silently operating on an incompatible or unrelated SQLite
 * file. `CREATE TABLE IF NOT EXISTS` alone will happily "succeed" against a
 * database with same-named tables but a different shape (or a future/older
 * objectify schema version), only to fail confusingly deep inside a query
 * later. This throws a clear, upfront error instead.
 */
function checkSchemaCompatibility(db: Database.Database, dbPath: string): void {
  const existingTables = new Set(
    (
      db
        .prepare(
          "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('objects','events')",
        )
        .all() as { name: string }[]
    ).map((r) => r.name),
  );

  if (existingTables.size === 0) {
    // Fresh database (or fresh file) — nothing to verify yet.
    return;
  }

  const incompatible = (reason: string): never => {
    throw new Error(`incompatible objectify database at "${dbPath}": ${reason}`);
  };

  if (!existingTables.has('objects') || !existingTables.has('events')) {
    incompatible(
      `expected both "objects" and "events" tables, found only [${[...existingTables].join(', ')}]. ` +
        `This file may not be an objectify database.`,
    );
  }

  for (const table of ['objects', 'events'] as const) {
    const actual = new Set(tableColumns(db, table));
    const missing = EXPECTED_COLUMNS[table].filter((c) => !actual.has(c));
    if (missing.length > 0) {
      incompatible(
        `"${table}" table exists but is missing expected column(s) [${missing.join(', ')}]. ` +
          `This file may belong to a different application, or to an incompatible version of objectify.`,
      );
    }
  }

  const version = db.pragma('user_version', { simple: true }) as number;
  if (version !== 0 && version !== SCHEMA_VERSION) {
    incompatible(
      `found schema version ${version}, expected ${SCHEMA_VERSION} (or 0, for a database ` +
        `predating schema versioning). This file may belong to a newer or otherwise ` +
        `incompatible version of objectify.`,
    );
  }
}

export function openDb(dbPath: string): Database.Database {
  const db = new Database(dbPath);
  db.pragma('journal_mode = WAL');
  db.pragma('foreign_keys = ON');
  checkSchemaCompatibility(db, dbPath);
  db.exec(SCHEMA_SQL);
  db.pragma(`user_version = ${SCHEMA_VERSION}`);
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
