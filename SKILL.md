---
name: objectify
description: >
  Use this skill whenever working with `objectify` — a CLI for persistent, versioned JSON objects
  backed by SQLite, designed for LLM agent use. Trigger this skill when the user asks to create,
  read, update, inspect, fork, or rewind objectify objects; write or register objectify classes
  in TypeScript or Python; use objectify as agent memory or state management; integrate objectify
  into an agentic workflow; or debug objectify commands. Also trigger when the user mentions
  durable objects, persistent agent state, or versioned JSON storage in the context of a local
  CLI tool.
---

# objectify skill

`objectify` is a CLI for persistent, versioned JSON objects backed by SQLite. Objects have optional
TypeScript or Python classes that define typed state and custom methods. Every write creates a new
version, enabling rewind and fork. Designed for LLM agents consuming it via bash.

Run `objectify --help` or `objectify <command> --help` for built-in docs, format references, and
examples directly from the binary.

---

## Key behaviors to remember

- **IDs are git-style**: stored as full random ULIDs, any unique prefix resolves. Always use the
  shortest unambiguous prefix when showing IDs to users.
- **stdout is always JSON**, stderr is always `{"error": "..."}`, exit 1 on failure.
- **`set` is full replacement** — never merged. Custom methods must read-then-write explicitly.
- **No registration** — drop a `.ts` or `.py` file in `.objectify/classes/`, objectify discovers
  it by filename. TypeScript takes precedence when both exist for the same name.
- **Local `.objectify/` takes precedence** over `~/.objectify/`. Both can coexist.
- **Schema validation** is derived from the `DoBase<T>` generic at class creation time. Invalid
  `set` calls fail loudly before writing anything.
- **History is append-only** — rewind writes a new version, never deletes.
- **Expired objects warn on stderr but still work** until `objectify gc` removes them.

---

## Full CLI reference

```sh
# Setup
objectify init                              # create .objectify/ in cwd
objectify init --global                     # create ~/.objectify/

# Objects
objectify create [description] [--class=X] [--expire=<duration>]
objectify destroy <id>
objectify inspect <id>
objectify list [--class=X] [--expired] [--since=<date>] [--limit=N] [--offset=N] [--json]

# Classes
objectify classes [--json]                  # list .ts/.py files with language + object count

# State
objectify use <id> get [--at=<version>]
objectify use <id> set <json>
objectify use <id> <method> [input]

# Method input forms (all equivalent)
objectify use <id> <method> '{"key": "value"}'      # positional JSON
objectify use <id> <method> -p '{"key": "value"}'   # -p flag, full JSON
objectify use <id> <method> -p:key value             # -p flag, kv pair
objectify use <id> <method> --parameter:key value    # long form
objectify use <id> <method> -p:key1 val -p:key2 val # multiple kv, merged

# History
objectify log <id>
objectify diff <id> <v1> <v2>              # RFC 6902 JSON Patch output
objectify rewind <id> <version>            # non-destructive, writes new version
objectify fork <id> [--at=<version>]       # returns new short ID

# Maintenance
objectify gc                               # delete expired objects
```

Expiry format: `30s`, `15m`, `2h`, `7d`, `2w` (`s` `m` `h` `d` `w`).

`--since` format: relative (`2d`, `1w`) or ISO 8601 date/datetime.

---

## Method input: `-p` flag

When calling class methods, input can be passed as positional JSON or via `-p`/`--parameter` flags.
All forms produce the same input to the method:

```sh
# These are all equivalent
objectify use 3fa8 add '{"title": "write tests", "priority": 1}'
objectify use 3fa8 add -p '{"title": "write tests", "priority": 1}'
objectify use 3fa8 add -p:title "write tests" -p:priority 1
objectify use 3fa8 add --parameter:title "write tests" --parameter:priority 1
```

Values in `-p:key VALUE` are JSON-parsed first (numbers, booleans, arrays, objects all work).
Falls back to a plain string if the value isn't valid JSON.

---

## Writing TypeScript classes

Classes live in `.objectify/classes/`. Filename = class name. Requires Deno at runtime.

```ts
import { DoBase } from 'objectify';

interface MyState { count: number; }

export default class Counter extends DoBase<MyState> {
  increment = async ({ by = 1 }: { by?: number } = {}) => {
    const { count = 0 } = await this.get();
    await this.set({ count: count + by });
    return count + by;
  };

  reset = async () => {
    await this.set({ count: 0 });
  };

  value = async () => {
    const { count = 0 } = await this.get();
    return count;
  };
}
```

**Rules:**
- Default export; class name matches filename
- Methods must be **arrow functions assigned to class fields** (not prototype methods) — required
  for correct `this` binding when the runner extracts methods by name
- `this.get()` / `this.set()` are the only state accessors — injected at runtime
- Return values are serialized to stdout automatically
- Throw to signal failure — objectify catches and writes to stderr
- Use `npm:package-name` imports — Deno fetches them automatically, no install step

---

## Writing Python classes

Classes live in `.objectify/classes/`. Filename = class name. Requires `python3` at runtime.
Supports **Pydantic models** and **dataclasses**. Methods can be `async def` or plain `def`.

```python
from __future__ import annotations
from objectify import DoBase
from pydantic import BaseModel

class CounterState(BaseModel):
    count: int = 0

class Counter(DoBase[CounterState]):
    async def increment(self, by: int = 1) -> int:
        state = await self.get() or {}
        new_count = state.get("count", 0) + by
        await self.set(CounterState(count=new_count))
        return new_count

    async def reset(self) -> None:
        await self.set(CounterState())

    async def value(self) -> int:
        state = await self.get() or {}
        return state.get("count", 0)
```

**Rules:**
- Class name matches filename
- Methods can be `async def` or plain `def` — both work
- `await self.get()` returns the raw state as a plain dict; deserialize as needed
- `await self.set(obj)` accepts a Pydantic model, dataclass, or plain dict
- **Dict inputs are unpacked as keyword arguments**: `'{"by": 2}'` calls `increment(by=2)`
- Scalar inputs are passed positionally
- Raise an exception to signal failure
- Python classes are **not sandboxed** — install dependencies with `pip` as normal

---

## TypeScript vs Python: key differences

| | TypeScript | Python |
|---|---|---|
| Runtime | Deno | python3 |
| Sandboxed | Yes (`class.json` sidecar) | No |
| Method style | Arrow function fields | `def` / `async def` |
| State type | `interface` / `type` | Pydantic model or `@dataclass` |
| Dict input | Received as-is | Unpacked as kwargs |
| npm/pip | `npm:` imports (auto-fetch) | `pip install` |
| Precedence | Takes priority over .py | Falls back when no .ts |

---

## Listing and finding classes

```sh
# List all available classes with language + object count
objectify classes

# JSON output for scripting
objectify classes --json | jq '.[].name'

# Find all objects using a specific class
objectify list --class=TaskList --json
```

---

## Agent usage patterns

### Checkpoint before risky operations
```sh
CHECKPOINT=$(objectify fork <id>)
# ... agent does risky multi-step work ...
# if it goes wrong:
objectify rewind <id> <version>
```

### Typed agent memory
```sh
objectify create "session memory" --class=Memory
# → a3f1
objectify use a3f1 store -p:content "user prefers dark mode" -p:tags '["prefs"]'
objectify use a3f1 recall -p:topK 3
```

### Shared state across agent steps
```sh
objectify create "pipeline state" --class=Pipeline
# → b2c9
objectify use b2c9 set '{"stage": "fetch", "items": []}'
# ... later in any step ...
objectify use b2c9 get
```

### Audit what changed
```sh
objectify log b2c9
objectify diff b2c9 1 5      # RFC 6902 patch across 5 steps
```

### Expiring temporary state
```sh
objectify create "session context" --class=Context --expire=2h
# → f3c1
# ... automatically warns on use after 2h ...
objectify gc                  # bulk-remove all expired objects
```

---

## When helping the user write a class

1. Ask what state shape they need — define the `interface` / `BaseModel` for `DoBase<T>` first
2. Ask which language they prefer (TypeScript/Deno or Python); default to TypeScript if unsure
3. Identify which operations are read-only (return from state, no `set`) vs mutating
4. For mutating methods: always read-then-write, never assume current state
5. Suggest forking for any destructive batch operation
6. For TypeScript: use `npm:` imports for packages — no setup needed with Deno
7. For Python: dict inputs unpack as kwargs, so design method signatures with named params

---

## Directory structure reference

```
.objectify/
  classes/
    TaskList.ts       ← TypeScript class (Deno; sandboxed)
    Counter.py        ← Python class (python3; unsandboxed)
    Memory.ts
    Memory.ts.json    ← optional: class permissions sidecar (TypeScript only)
  deno.json           ← optional: Deno import map / npm package declarations
  deno.lock
  objectify.db        ← SQLite; all objects + full event history
```
