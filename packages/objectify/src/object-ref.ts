import type Database from 'better-sqlite3';
import _Ajv from 'ajv';
const Ajv = _Ajv as unknown as typeof _Ajv.default;

import fastJsonPatch from 'fast-json-patch';
const { compare } = fastJsonPatch;
import {
  displayId,
  getObject,
  getStateAt,
  writeEvent,
  versionCount,
} from './db.js';
import { newId } from './id.js';
import { runClassMethod } from './runner.js';
import type { LogEntry } from './types.js';

const ajv = new Ajv();

export class ObjectRef {
  readonly id: string;
  readonly shortId: string;

  constructor(
    private db: Database.Database,
    fullId: string,
    private classesDir: string,
  ) {
    this.id = fullId;
    this.shortId = displayId(db, fullId);
  }

  get<T = unknown>(version?: number): T {
    return getStateAt(this.db, this.id, version) as T;
  }

  set(state: unknown): void {
    const obj = getObject(this.db, this.id);
    if (obj.schema) {
      const schema = JSON.parse(obj.schema);
      const validate = ajv.compile(schema);
      if (!validate(state)) {
        const msgs = (validate.errors ?? []).map((e: { message?: string }) => e.message).join('; ');
        throw new Error(`schema validation failed: ${msgs}`);
      }
    }
    writeEvent(this.db, this.id, 'set', state);
  }

  log(): LogEntry[] {
    const rows = this.db
      .prepare(
        'SELECT version, method, created_at FROM events WHERE object_id = ? ORDER BY version ASC',
      )
      .all(this.id) as { version: number; method: string; created_at: string }[];

    return rows.map((r) => ({
      version: r.version,
      method: r.method,
      at: r.created_at,
    }));
  }

  diff(v1: number, v2: number): unknown {
    const state1 = getStateAt(this.db, this.id, v1);
    const state2 = getStateAt(this.db, this.id, v2);
    return compare(
      state1 as Record<string, unknown>,
      state2 as Record<string, unknown>,
    );
  }

  rewind(version: number): { rewoundTo: number; newVersion: number } {
    const state = getStateAt(this.db, this.id, version);
    const newVersion = writeEvent(this.db, this.id, 'rewind', state);
    return { rewoundTo: version, newVersion };
  }

  fork(opts?: { at?: number }): string {
    const obj = getObject(this.db, this.id);
    const state = getStateAt(this.db, this.id, opts?.at);
    const forkId = newId();
    const now = new Date().toISOString();

    this.db
      .prepare(
        'INSERT INTO objects (id, class, description, schema, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?)',
      )
      .run(forkId, obj.class, obj.description, obj.schema, now, obj.expiresAt);

    writeEvent(this.db, forkId, 'create', state);
    return displayId(this.db, forkId);
  }

  inspect() {
    const obj = getObject(this.db, this.id);
    return {
      id: obj.id,
      shortId: this.shortId,
      class: obj.class,
      description: obj.description,
      versions: versionCount(this.db, this.id),
      createdAt: obj.createdAt,
      expiresAt: obj.expiresAt,
    };
  }

  call(method: string, input?: unknown): unknown {
    const obj = getObject(this.db, this.id);
    if (!obj.class) {
      throw new Error(
        `object has no class; cannot call method '${method}'`,
      );
    }

    const currentState = (() => {
      try {
        return getStateAt(this.db, this.id);
      } catch {
        return null;
      }
    })();

    const result = runClassMethod({
      classesDir: this.classesDir,
      className: obj.class,
      method,
      state: currentState,
      input,
    });

    if (result.stateChanged && result.state !== null) {
      if (obj.schema) {
        const schema = JSON.parse(obj.schema);
        const validate = ajv.compile(schema);
        if (!validate(result.state)) {
          const msgs = (validate.errors ?? [])
            .map((e: { message?: string }) => e.message)
            .join('; ');
          throw new Error(`schema validation failed: ${msgs}`);
        }
      }
      writeEvent(this.db, this.id, method, result.state);
    }

    return result.result;
  }
}
