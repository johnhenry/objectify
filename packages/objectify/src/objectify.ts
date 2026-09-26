import { existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { homedir } from 'node:os';
import type Database from 'better-sqlite3';
import {
  openDb,
  resolveId,
  displayId,
  getObject,
  writeEvent,
  versionCount,
  parseExpiryDuration,
} from './db.js';
import { newId } from './id.js';
import { ObjectRef } from './object-ref.js';
import type {
  ObjectifyOptions,
  CreateOptions,
  ListOptions,
  InspectResult,
} from './types.js';

function findDir(): string {
  let d = process.cwd();
  while (true) {
    const candidate = join(d, '.objectify');
    if (existsSync(candidate)) return candidate;
    const parent = dirname(d);
    if (parent === d) break;
    d = parent;
  }
  const global = join(homedir(), '.objectify');
  if (existsSync(global)) return global;
  throw new Error('no .objectify/ found. Run: objectify init');
}

export class Objectify {
  private db: Database.Database;
  private dir: string;
  private classesDir: string;

  constructor(opts?: ObjectifyOptions) {
    this.dir = opts?.dir ?? findDir();
    this.classesDir = join(this.dir, 'classes');
    const dbPath = join(this.dir, 'objectify.db');
    this.db = openDb(dbPath);
  }

  create(opts?: CreateOptions): string {
    const id = newId();
    const now = new Date().toISOString();
    const expiresAt = opts?.expire
      ? parseExpiryDuration(opts.expire)
      : null;

    this.db
      .prepare(
        'INSERT INTO objects (id, class, description, schema, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?)',
      )
      .run(id, opts?.class ?? null, opts?.description ?? null, null, now, expiresAt);

    writeEvent(this.db, id, 'create', null);
    return displayId(this.db, id);
  }

  destroy(idPrefix: string): void {
    const id = resolveId(this.db, idPrefix);
    this.db.prepare('DELETE FROM objects WHERE id = ?').run(id);
  }

  use(idPrefix: string): ObjectRef {
    const id = resolveId(this.db, idPrefix);
    return new ObjectRef(this.db, id, this.classesDir);
  }

  inspect(idPrefix: string): InspectResult {
    const id = resolveId(this.db, idPrefix);
    const obj = getObject(this.db, id);
    return {
      id: obj.id,
      shortId: displayId(this.db, id),
      class: obj.class,
      description: obj.description,
      versions: versionCount(this.db, id),
      createdAt: obj.createdAt,
      expiresAt: obj.expiresAt,
    };
  }

  list(opts?: ListOptions): InspectResult[] {
    const limit = opts?.limit ?? 50;
    const offset = opts?.offset ?? 0;
    const conditions: string[] = [];
    const params: unknown[] = [];

    if (!opts?.expired) {
      conditions.push(
        "(expires_at IS NULL OR expires_at > datetime('now'))",
      );
    }
    if (opts?.class) {
      conditions.push('class = ?');
      params.push(opts.class);
    }
    if (opts?.since) {
      conditions.push('created_at >= ?');
      params.push(opts.since);
    }

    const where =
      conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';
    const sql = `SELECT id, class, description, schema, created_at, expires_at FROM objects ${where} ORDER BY created_at DESC LIMIT ? OFFSET ?`;
    params.push(limit, offset);

    const rows = this.db.prepare(sql).all(...params) as {
      id: string;
      class: string | null;
      description: string | null;
      schema: string | null;
      created_at: string;
      expires_at: string | null;
    }[];

    return rows.map((r) => ({
      id: r.id,
      shortId: displayId(this.db, r.id),
      class: r.class,
      description: r.description,
      versions: versionCount(this.db, r.id),
      createdAt: r.created_at,
      expiresAt: r.expires_at,
    }));
  }

  gc(): number {
    const now = new Date().toISOString();
    const result = this.db
      .prepare(
        'DELETE FROM objects WHERE expires_at IS NOT NULL AND expires_at <= ?',
      )
      .run(now);
    return result.changes;
  }

  close(): void {
    this.db.close();
  }
}
