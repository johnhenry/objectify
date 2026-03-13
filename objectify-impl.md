# objectify — Implementation Spec

## Architecture overview

Rust binary for everything except class method execution. Deno is an optional runtime dependency invoked as a subprocess only when a user-defined class method is called. The common agent path — `get`, `set`, `list`, `log`, `rewind`, `fork` — never touches Deno.

```
┌─────────────────────────────────────────────┐
│                objectify (Rust)              │
│                                              │
│  CLI parsing       clap                      │
│  ID management     custom (see below)        │
│  SQLite            rusqlite                  │
│  Schema validation jsonschema                │
│  JSON diff         json-patch                │
│  Expiry/GC         built-in                  │
│                                              │
│              ┌───────────────┐               │
│              │  class method │               │
│              │  execution    │               │
│              │               │               │
│              │  deno run     │  subprocess   │
│              │  (optional)   │  only         │
│              └───────────────┘               │
└─────────────────────────────────────────────┘
```

---

## Rust crates

```toml
[dependencies]
clap            = { version = "4", features = ["derive"] }
rusqlite        = { version = "0.31", features = ["bundled"] }
serde           = { version = "1", features = ["derive"] }
serde_json      = "1"
jsonschema      = "0.18"
json-patch      = "1"
ulid            = "1"          # for ID generation (see below)
chrono          = { version = "0.4", features = ["serde"] }
thiserror       = "1"
anyhow          = "1"
which           = "6"          # locate deno binary at runtime
tempfile        = "3"          # staging class execution context
```

`rusqlite` with `bundled` feature links SQLite statically — no system SQLite dependency, single binary.

---

## ID scheme

ULIDs rather than UUIDs. Monotonically sortable, URL-safe, and the character set (Crockford base32) is more prefix-friendly than hex. A ULID is 26 characters — the minimum unique prefix in practice will usually be 4–6 chars.

Resolution algorithm:
1. Query `SELECT id FROM objects WHERE id LIKE ?1 || '%'`
2. If 0 results → error: not found
3. If 1 result → resolved
4. If 2+ results → error: ambiguous, list matches

Short IDs in all output are the minimum prefix that currently resolves uniquely, recomputed at display time.

---

## SQLite schema

```sql
CREATE TABLE objects (
  id          TEXT PRIMARY KEY,
  class       TEXT,
  description TEXT,
  schema      TEXT,           -- JSON Schema string, nullable (base objects)
  created_at  TEXT NOT NULL,
  expires_at  TEXT            -- ISO 8601, nullable
);

CREATE TABLE events (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  object_id   TEXT NOT NULL REFERENCES objects(id) ON DELETE CASCADE,
  version     INTEGER NOT NULL,
  method      TEXT NOT NULL,  -- 'create' | 'set' | <method name>
  state       TEXT NOT NULL,  -- full JSON snapshot
  created_at  TEXT NOT NULL,
  UNIQUE(object_id, version)
);

CREATE INDEX idx_events_object_version ON events(object_id, version);
CREATE INDEX idx_objects_class ON objects(class);
CREATE INDEX idx_objects_expires ON objects(expires_at) WHERE expires_at IS NOT NULL;
```

Full snapshots per event row. No delta storage. History is always directly readable without reconstruction — simplicity and debuggability outweigh storage cost.

---

## Directory resolution

At startup, objectify walks up from cwd looking for `.objectify/`. If found, that's the active context. If not found anywhere in the tree, falls back to `~/.objectify/`. If neither exists, most commands error with a helpful message pointing to `objectify init`.

```rust
fn resolve_context() -> Result<PathBuf> {
    // walk cwd upward
    let mut dir = std::env::current_dir()?;
    loop {
        let candidate = dir.join(".objectify");
        if candidate.is_dir() {
            return Ok(candidate);
        }
        if !dir.pop() { break; }
    }
    // fall back to global
    let global = dirs::home_dir()
        .ok_or_else(|| anyhow!("cannot resolve home dir"))?
        .join(".objectify");
    if global.is_dir() {
        return Ok(global);
    }
    Err(anyhow!("no .objectify/ found. Run: objectify init"))
}
```

---

## Class execution — Rust → Deno boundary

When `objectify use <id> <method> [input]` is called and the object has a class:

1. Rust reads the class file from `.objectify/classes/<ClassName>.ts`
2. Rust reads current state from SQLite
3. Rust writes a temporary runner script (see below)
4. Rust spawns `deno run --allow-read --allow-net=... <runner>` with input on stdin
5. Deno executes the method, writes result to stdout
6. Rust reads stdout, validates result, writes new event row if state changed
7. Temp file cleaned up

### Runner script template

Rust generates this per invocation:

```ts
import UserClass from '/abs/path/to/ClassName.ts';

const input = JSON.parse(Deno.env.get('OBJECTIFY_INPUT') || 'null');
const currentState = JSON.parse(Deno.env.get('OBJECTIFY_STATE') || '{}');

let pendingState: unknown = currentState;
let stateChanged = false;

const instance = new UserClass();

// inject get/set
(instance as any).get = async () => structuredClone(currentState);
(instance as any).set = async (next: unknown) => {
  pendingState = next;
  stateChanged = true;
};

const result = await (instance as any)[Deno.env.get('OBJECTIFY_METHOD')!](input);

const output = {
  result: result ?? null,
  stateChanged,
  state: stateChanged ? pendingState : null,
};

console.log(JSON.stringify(output));
```

State and input passed via environment variables rather than stdin to keep stdin available for user code that might need it.

### Deno permissions

Objectify always grants a baseline set of permissions, then extends based on a `class.json` sidecar file next to the `.ts` file.

**Baseline (always granted, no configuration needed):**
- `--allow-read=<classes-dir>,<deno-cache-dir>` — class files and Deno's module cache
- `--allow-env=OBJECTIFY_INPUT,OBJECTIFY_STATE,OBJECTIFY_METHOD` — only the vars objectify sets

Everything else requires explicit declaration in `class.json`.

---

### class.json sidecar format

```json
{
  "net": true | ["host:port", "host", ...],
  "read": true | ["/path/one", "/path/two", ...],
  "write": true | ["/path/one", "/path/two", ...],
  "env": true | ["VAR_ONE", "VAR_TWO", ...],
  "run": true | ["binary-name", ...],
  "sys": ["osRelease", "hostname", ...]
}
```

Every field is optional and absent by default (denied). `true` grants the Deno wildcard for that permission (e.g. `--allow-net`). An array grants only the listed values.

**Examples:**

Class that calls the OpenAI API and reads local files:
```json
{
  "net": ["api.openai.com"],
  "env": ["OPENAI_API_KEY"],
  "read": ["/home/user/documents"]
}
```

Class with full network access and specific env vars:
```json
{
  "net": true,
  "env": ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "DATABASE_URL"]
}
```

Class that writes to a scratch directory:
```json
{
  "write": ["/tmp/objectify-scratch"],
  "read": ["/tmp/objectify-scratch"]
}
```

Class that shells out to a binary:
```json
{
  "run": ["ffmpeg", "convert"],
  "read": true,
  "write": ["/tmp"]
}
```

---

### Path tokens

A small set of tokens are expanded in `read`/`write` paths to avoid hardcoded absolute paths:

| Token | Expands to |
|---|---|
| `$HOME` | User's home directory |
| `$CWD` | Directory where objectify was invoked |
| `$OBJECTIFY_DIR` | The active `.objectify/` directory |
| `$TMPDIR` | System temp directory |

```json
{
  "read": ["$HOME/documents", "$CWD/data"],
  "write": ["$TMPDIR/objectify-scratch"]
}
```

---

### Rust permission assembly

Rust reads `class.json` (if present) and assembles the final flag list before spawning:

```rust
fn build_deno_flags(ctx: &ObjectifyContext, class_name: &str) -> Vec<String> {
    let mut flags = vec![
        format!("--allow-read={},{}", ctx.classes_dir.display(), deno_cache_dir().display()),
        format!("--allow-env=OBJECTIFY_INPUT,OBJECTIFY_STATE,OBJECTIFY_METHOD"),
    ];

    let sidecar = ctx.classes_dir.join(format!("{}.json", class_name));
    if let Ok(raw) = fs::read_to_string(&sidecar) {
        let perms: ClassPermissions = serde_json::from_str(&raw)
            .unwrap_or_else(|e| {
                print_error(format!("invalid {}.json: {}", class_name, e));
            });

        if let Some(net) = perms.net {
            flags.push(match net {
                Permission::All => "--allow-net".into(),
                Permission::List(hosts) => format!("--allow-net={}", hosts.join(",")),
            });
        }
        if let Some(read) = perms.read {
            // expand tokens, merge with baseline read paths
            let paths = expand_paths(read, ctx);
            flags.push(format!("--allow-read={},{}", 
                ctx.classes_dir.display(), 
                paths.join(",")));
        }
        if let Some(write) = perms.write {
            let paths = expand_paths(write, ctx);
            flags.push(format!("--allow-write={}", paths.join(",")));
        }
        if let Some(env) = perms.env {
            let base = "OBJECTIFY_INPUT,OBJECTIFY_STATE,OBJECTIFY_METHOD";
            flags.push(match env {
                Permission::All => "--allow-env".into(),
                Permission::List(vars) => format!("--allow-env={},{}", base, vars.join(",")),
            });
        }
        if let Some(run) = perms.run {
            flags.push(match run {
                Permission::All => "--allow-run".into(),
                Permission::List(bins) => format!("--allow-run={}", bins.join(",")),
            });
        }
        if let Some(sys) = perms.sys {
            flags.push(format!("--allow-sys={}", sys.join(",")));
        }
    }

    flags
}
```

---

### Security model

`class.json` is a user-controlled file in the `.objectify/` directory — objectify makes no attempt to audit or restrict what permissions are declared. The model is transparency, not sandboxing: permissions are explicit and visible, not implicit. If an agent is writing class files dynamically, that is a user concern, not objectify's.

### Deno availability

At startup for any class-involving command, objectify calls `which::which("deno")`. If not found:

```
error: class execution requires Deno. Install from https://deno.land
```

For commands that don't involve class execution, Deno absence is silently ignored.

---

## Schema extraction

When a class file is first used (at `objectify create --class=X`), objectify runs a one-time schema extraction:

```sh
deno run --allow-read <extractor.ts> <ClassName.ts>
```

The extractor uses `npm:ts-json-schema-generator` to pull the generic parameter `T` from `DoBase<T>` and emit a JSON Schema. Objectify stores this in `objects.schema`.

On every subsequent `set` (direct or via method), Rust validates the incoming state JSON against the stored schema using `jsonschema` crate before writing the event row.

If the class file changes (mtime check), schema is re-extracted on next use.

---

## Expiry and GC

Objectify does not run a background daemon. GC is lazy:

- On every `objectify list`, expired objects are marked but still shown unless `--expired` is omitted
- A `objectify gc` command permanently deletes all expired objects and their events
- Expired objects respond to `get`/`log` etc. with a warning header in stderr but still function

This keeps the tool simple — no cron, no daemon, no pid files.

```
$ objectify use cc01 get
# stderr: warning: object cc01 expired 2 hours ago
# stdout: {"count": 3}
```

---

## Output contract

All stdout is valid JSON. All errors go to stderr as `{"error": "..."}`. Exit 0 on success, exit 1 on error.

```rust
fn print_success<T: Serialize>(val: T) {
    println!("{}", serde_json::to_string(&val).unwrap());
    std::process::exit(0);
}

fn print_error(msg: impl Display) -> ! {
    eprintln!("{}", serde_json::json!({"error": msg.to_string()}));
    std::process::exit(1);
}
```

Table output for human-facing commands (`list`, `log`, `inspect`) is suppressed when `--json` is passed or when stdout is not a TTY — so piping always gets JSON automatically without requiring `--json`.

---

## Binary distribution

`cargo build --release` produces a single static binary (SQLite bundled). Distribution options in priority order:

1. `cargo install objectify` — for Rust users
2. Prebuilt binaries via GitHub releases (x86_64-linux, aarch64-linux, x86_64-darwin, aarch64-darwin)
3. `deno install` shim if a pure-Deno fallback mode is ever added

The binary has no runtime dependencies except an optional `deno` on PATH for class execution.
