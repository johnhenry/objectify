---
title: "Output contract, SQLite schema & architecture"
---

## Output contract

- All stdout is valid JSON (even single values like short IDs)
- All errors go to stderr as `{"error": "message"}`
- Exit 0 on success, exit 1 on error
- `--json` is available on `list` to force JSON output regardless of TTY
- `log` and `list` auto-detect TTY: piping always gets JSON
- Expired object warnings go to stderr; the operation still proceeds
- `objectify use <id> <method>` input flags (`-p`, `--parameter`, `-p:key`, `--parameter:key`) are consumed before the method is called and never passed to the class

---

## SQLite schema

```sql
CREATE TABLE objects (
  id          TEXT PRIMARY KEY,  -- random 32-char hex
  class       TEXT,
  description TEXT,
  schema      TEXT,              -- JSON Schema, nullable
  created_at  TEXT NOT NULL,
  expires_at  TEXT               -- ISO 8601, nullable
);

CREATE TABLE events (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  object_id   TEXT NOT NULL REFERENCES objects(id) ON DELETE CASCADE,
  version     INTEGER NOT NULL,
  method      TEXT NOT NULL,     -- 'create' | 'set' | method name
  state       TEXT NOT NULL,     -- full JSON snapshot
  created_at  TEXT NOT NULL,
  UNIQUE(object_id, version)
);
```

Every write is a full snapshot. No delta storage — history is always directly readable without reconstruction.

---

## Architecture

```
┌─────────────────────────────────────────────┐
│                objectify (Rust)              │
│                                              │
│  CLI parsing       clap                      │
│  ID management     ULID + prefix resolution  │
│  SQLite            rusqlite (bundled)        │
│  Schema validation jsonschema                │
│  JSON diff         json-patch (RFC 6902)     │
│  Expiry/GC         built-in lazy GC          │
│                                              │
│  ┌──────────────────┐  ┌──────────────────┐ │
│  │  TypeScript class│  │  Python class    │ │
│  │  deno run        │  │  python3         │ │
│  │  (sandboxed)     │  │  (unsandboxed)   │ │
│  └──────────────────┘  └──────────────────┘ │
└─────────────────────────────────────────────┘
```

Class subprocesses are only invoked when calling a user-defined method. The common agent path — `get`, `set`, `list`, `log`, `rewind`, `fork` — never starts a subprocess.

SQLite is linked statically (via `rusqlite` bundled feature). The binary has no runtime dependencies beyond optional `deno` or `python3` for class execution.

---

## License

MIT
