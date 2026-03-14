# objectify

Turn a TypeScript or Python class into a CLI tool. Instantly.

Write a class. Drop it in a folder. Every method becomes a shell command. Every call takes JSON arguments. Every write is versioned. State is persistent, optional, and backed by SQLite. The whole thing is a single binary — no server, no SDK, no configuration.

objectify gives LLM agents the easiest possible way to create and use tools. An agent writes a class, and immediately has a stateful, versioned, self-documenting CLI it can call through bash. No boilerplate, no glue code, no infrastructure. Just classes in, tools out.

It's also a pretty great way for humans to build CLI tools without all the argument parsing.

```sh
objectify create "sprint tasks" --class=TaskList
# → 3fa8

objectify use 3fa8 add -p:title "write tests"
# → {"id": "9a3f", "title": "write tests", "done": false, ...}

objectify use 3fa8 pending
# → [{"id": "9a3f", "title": "write tests", "done": false}]

objectify log 3fa8
# VERSION  METHOD    AT
# 1        create    2 hours ago
# 2        add       1 hour ago
```

---

## Installation

Requires [Rust](https://rustup.rs) 1.70+.

```sh
git clone <repo>
cd objectify
cargo build --release
# binary at: target/release/objectify
```

### Optional: Deno (for TypeScript classes)

Required only for TypeScript class method execution.

```sh
curl -fsSL https://deno.land/install.sh | sh
```

### Optional: Python 3 (for Python classes)

Required only for Python class method execution. Python 3.10+ recommended.

```sh
# macOS
brew install python

# Linux
apt install python3
```

For schema extraction from typed Python classes, install Pydantic:

```sh
pip install pydantic
```

---

## Quick start

```sh
# Create a local .objectify/ directory in your project
objectify init

# Or initialize globally (available everywhere)
objectify init --global

# Create an untyped object
objectify create "my config"
# → a1b2

# Write state (full replacement)
objectify use a1b2 set '{"theme": "dark", "fontSize": 14}'

# Read state
objectify use a1b2 get
# → {"theme": "dark", "fontSize": 14}

# See history
objectify log a1b2

# Rewind to version 1 (non-destructive)
objectify rewind a1b2 1
```

---

## IDs

Every object gets a random 32-character hex ID at creation. You never need to type the whole thing — any unique prefix resolves to the full ID, same model as git. The shortest unambiguous prefix is shown in all output (usually 4 characters).

```sh
objectify create "my thing"
# → a3f1

objectify use a3f1 get     # works
objectify use a3 get       # works if unique
objectify use a get        # error if ambiguous
```

Because IDs are fully random, even short prefixes are almost always unique. With a few hundred objects, 4 characters gives you 65,536 possible prefixes — collisions are rare.

---

## Directory resolution

`objectify` looks for `.objectify/` walking up from the current directory, then falls back to `~/.objectify/`. Local always wins. Classes defined in either location are available to all objects in that scope.

Both TypeScript and Python classes live together in the same `classes/` directory:

```
.objectify/
  classes/
    TaskList.ts       ← TypeScript class (requires Deno)
    TaskList.py       ← Python class (requires python3; .ts takes precedence if both exist)
    Memory.ts
    Counter.py
  deno.json           ← optional: Deno import map / npm package declarations
  deno.lock
  objectify.db        ← SQLite database; all objects + full event history
```

---

## CLI reference

Run `objectify --help` or `objectify <command> --help` for built-in documentation including examples. Every command has a `--help` flag with a description, per-argument docs, format reference, and examples.

| Command | Description |
|---------|-------------|
| `init` | Create `.objectify/` in cwd or `~/.objectify/` globally |
| `create` | Create a new object, optionally with a class and expiry |
| `destroy` | Permanently delete an object and its history |
| `inspect` | Show object metadata as JSON |
| `list` | List objects with optional filters |
| `classes` | List available classes with language and object count |
| `classes <name>` | Describe methods on a specific class |
| `use … get` | Read current state (or a pinned version) |
| `use … set` | Replace state entirely |
| `use … help` | List available methods on the object's class |
| `use … <method>` | Call a class method |
| `log` | Version history for an object |
| `diff` | RFC 6902 JSON Patch between two versions |
| `rewind` | Restore to a previous version (non-destructive) |
| `fork` | Copy an object into a new independent object |
| `gc` | Delete all expired objects |

### `objectify init`

```sh
objectify init             # creates .objectify/ in cwd
objectify init --global    # creates ~/.objectify/
```

### `objectify create`

```sh
objectify create [description] [--class=ClassName] [--expire=<duration>]
```

Creates a new object and prints its short ID.

```sh
objectify create
# → a1b2

objectify create "sprint tasks" --class=TaskList
# → 3fa8

objectify create "temp flags" --expire=7d
# → cc01
```

Expiry format: `s`, `m`, `h`, `d`, `w` — e.g. `30s`, `15m`, `2h`, `7d`, `2w`.

### `objectify destroy`

```sh
objectify destroy <id>
```

Permanently deletes the object and all its version history (CASCADE delete). Irreversible — use `objectify gc` for bulk expiry cleanup instead.

### `objectify inspect`

```sh
objectify inspect <id>
```

```json
{
  "id": "3fa8c291d6b0...",
  "shortId": "3fa8",
  "class": "TaskList",
  "description": "sprint tasks",
  "versions": 4,
  "createdAt": "2025-03-10T14:22:00Z",
  "expiresAt": null
}
```

### `objectify list`

```sh
objectify list [options]
```

| Option | Description |
|--------|-------------|
| `--class=ClassName` | Filter by class |
| `--expired` | Include expired objects (hidden by default) |
| `--since=<date\|duration>` | Created on or after (ISO date or relative: `2d`, `1w`) |
| `--limit=N` | Default 50 |
| `--offset=N` | For pagination |
| `--json` | Force JSON output |

When stdout is a TTY, outputs a human-readable table. Otherwise (pipe, redirect, `--json`), outputs a JSON array.

```
ID      CLASS       DESCRIPTION            VER  CREATED          EXPIRES
3fa8    TaskList    sprint tasks           4    2 hours ago      never
cc01    -           temp flags             1    5 mins ago       in 7 days
a1b2    Memory      project memory         12   3 days ago       never
```

### `objectify use <id> get`

```sh
objectify use <id> get [--at=<version>]
```

Returns the current state as JSON. `--at` accepts a version number.

```sh
objectify use 3fa8 get
# → {"tasks": [{"id": "9a3f", "title": "write tests", "done": false}]}

objectify use 3fa8 get --at=1
# → null
```

### `objectify use <id> set`

```sh
objectify use <id> set <json>
```

Full replacement. Validates against the class schema if one is stored. Writes a new version on success.

```sh
objectify use a1b2 set '{"count": 42}'
```

> **Note:** `set` is always a complete replacement — there is no merge. Read state first if you need to patch a field.

### `objectify use <id> <method>`

```sh
objectify use <id> <method> [input]
```

Calls a method defined on the object's class. Requires Deno (TypeScript) or Python 3 (Python).

Input can be provided in any of the following forms — all produce the same result:

```sh
# Positional JSON
objectify use 3fa8 add '{"title": "write tests"}'

# Full JSON via flag
objectify use 3fa8 add --parameter '{"title": "write tests"}'
objectify use 3fa8 add -p '{"title": "write tests"}'

# Key-value pairs (value is JSON-parsed; falls back to string)
objectify use 3fa8 add --parameter:title "write tests"
objectify use 3fa8 add -p:title "write tests"

# Multiple key-value pairs merged into one object
objectify use 3fa8 add -p:title "write tests" -p:priority 1 -p:done false
```

Values in `-p:key VALUE` are JSON-parsed first — numbers, booleans, arrays, and objects all work. Anything that isn't valid JSON is passed through as a plain string.

```sh
objectify use 3fa8 pending
# → [{"id": "9a3f", "title": "write tests", "done": false}]
```

### `objectify log`

```sh
objectify log <id>
```

```
VERSION  METHOD    AT
1        create    3 days ago
2        add       2 days ago
3        complete  1 hour ago
4        add       5 mins ago
```

When piped, outputs a JSON array: `[{"version": 1, "method": "create", "at": "..."}]`.

### `objectify diff`

```sh
objectify diff <id> <v1> <v2>
```

RFC 6902 JSON Patch output between two versions.

```sh
objectify diff 3fa8 1 3
# → [{"op": "add", "path": "/tasks/0", "value": {...}}]
```

### `objectify rewind`

```sh
objectify rewind <id> <version>
```

Restores state to a previous version. Non-destructive — writes a new version record. History is never deleted.

```sh
objectify rewind 3fa8 2
# → {"rewoundTo": 2, "newVersion": 5}
```

### `objectify fork`

```sh
objectify fork <id> [--at=<version>]
```

Creates a new independent object with the same state, class, description, and schema. Returns the new short ID. After forking, mutations to either object do not affect the other.

```sh
objectify fork 3fa8
# → b9c1

objectify fork 3fa8 --at=2
# → d4e2
```

### `objectify gc`

```sh
objectify gc
```

Permanently deletes all expired objects and their history. Expired objects continue to respond (with a stderr warning) until gc is run.

```json
{"deleted": 3}
```

---

## Listing classes

### `objectify classes`

```sh
objectify classes [--json]
```

Lists all classes defined in the active `.objectify/classes/` directory. Shows the class name, runtime language, filename, and how many objects are currently using each class. When stdout is a TTY, outputs a table; otherwise JSON.

```
NAME                 LANG         OBJECTS  FILE
Counter              Python       0        Counter.py
Memory               Python       1        Memory.py
Memory               TypeScript   1        Memory.ts
TaskList             TypeScript   2        TaskList.ts
```

If both a `.ts` and `.py` file exist for the same class name, both are listed — but TypeScript takes precedence at runtime.

```sh
# Find all objects using a specific class
objectify list --class=TaskList

# JSON output for scripting
objectify classes --json | jq '.[].name'
```

Classes are discovered by filename at runtime — drop a `.ts` or `.py` file in `classes/` and it's immediately available. No registration or restart needed.

---

## Writing classes

Classes live in `.objectify/classes/` or `~/.objectify/classes/`. The filename is the class name. If both a `.ts` and a `.py` file exist for the same name, TypeScript takes precedence.

objectify supports two runtimes:

| Feature | TypeScript (Deno) | Python 3 |
|---------|-------------------|----------|
| Runtime | `deno run` | `python3` / `python` |
| Sandboxed | Yes (`class.json` for permissions) | No |
| Async methods | Yes (`async` arrow functions) | Yes (`async def`) |
| Sync methods | Yes | Yes |
| State type | `interface` / `type` | Pydantic model or `@dataclass` |
| Schema extraction | `ts-json-schema-generator` via Deno | Pydantic `model_json_schema()` |
| Dict input unpacking | No (receives raw input value) | Yes (`{"a":1}` → `method(a=1)`) |
| npm packages | `npm:package-name` imports | `pip install` as normal |

### TypeScript

```ts
// .objectify/classes/TaskList.ts
import { DoBase } from 'objectify';

interface Task {
  id: string;
  title: string;
  done: boolean;
  createdAt: string;
}

interface TaskListState {
  tasks: Task[];
}

export default class TaskList extends DoBase<TaskListState> {
  add = async ({ title }: { title: string }) => {
    const { tasks = [] } = await this.get();
    const task: Task = {
      id: crypto.randomUUID(),
      title,
      done: false,
      createdAt: new Date().toISOString(),
    };
    await this.set({ tasks: [...tasks, task] });
    return task;
  };

  complete = async ({ id }: { id: string }) => {
    const { tasks } = await this.get();
    await this.set({
      tasks: tasks.map(t => (t.id === id ? { ...t, done: true } : t)),
    });
  };

  pending = async () => {
    const { tasks } = await this.get();
    return tasks.filter(t => !t.done);
  };

  clear = async () => {
    await this.set({ tasks: [] });
  };
}
```

**TypeScript rules:**

- Default export; class name must match the filename
- Methods must be **arrow functions assigned to class fields** (not prototype methods) — this is what makes `this` bind correctly when the runner extracts the method by name at runtime
- `this.get()` and `this.set()` are the only state accessors — injected at runtime
- Return values are serialized to stdout automatically
- Throw to signal failure — objectify catches and writes to stderr
- Use `npm:package-name` for npm packages — Deno fetches them automatically

### Python

Supports both **Pydantic models** and **dataclasses** as the state type. Methods can be `async def` or plain `def`.

```python
# .objectify/classes/TaskList.py
from __future__ import annotations
from objectify import DoBase
from pydantic import BaseModel
import uuid
from datetime import datetime, timezone

class Task(BaseModel):
    id: str
    title: str
    done: bool
    created_at: str

class TaskListState(BaseModel):
    tasks: list[Task] = []

class TaskList(DoBase[TaskListState]):
    async def add(self, title: str) -> Task:
        state = await self.get()          # returns a plain dict
        task = Task(
            id=str(uuid.uuid4()),
            title=title,
            done=False,
            created_at=datetime.now(timezone.utc).isoformat(),
        )
        tasks = state.get("tasks", []) if state else []
        new_state = TaskListState(tasks=[Task(**t) for t in tasks] + [task])
        await self.set(new_state)         # Pydantic model auto-serialized
        return task

    async def complete(self, id: str) -> None:
        state = await self.get() or {}
        tasks = [Task(**t) for t in state.get("tasks", [])]
        await self.set(TaskListState(
            tasks=[t.model_copy(update={"done": True}) if t.id == id else t for t in tasks]
        ))

    async def pending(self) -> list[Task]:
        state = await self.get() or {}
        return [Task(**t) for t in state.get("tasks", []) if not t["done"]]
```

Using `dataclasses` instead of Pydantic:

```python
# .objectify/classes/Counter.py
from __future__ import annotations
from objectify import DoBase
from dataclasses import dataclass, field

@dataclass
class CounterState:
    value: int = 0
    history: list[int] = field(default_factory=list)

class Counter(DoBase[CounterState]):
    async def increment(self, by: int = 1) -> int:
        state = await self.get() or {}
        new_val = state.get("value", 0) + by
        hist = state.get("history", [])
        await self.set(CounterState(value=new_val, history=[*hist, new_val]))
        return new_val

    async def reset(self) -> None:
        await self.set(CounterState())
```

**Python rules:**

- Class name must match the filename
- Methods can be `async def` or plain `def` — both work
- `await self.get()` returns the raw state as a plain dict (not a typed model); deserialize as needed
- `await self.set(obj)` accepts a Pydantic model, dataclass, or plain dict — all serialized to JSON automatically
- **Dict inputs are unpacked as keyword arguments**: passing `'{"title": "x"}'` calls `add(title="x")`, so method signatures use named parameters directly
- Scalar inputs (strings, numbers) are passed positionally
- Raise an exception to signal failure

### Calling methods with input

The same call works identically regardless of whether the class is TypeScript or Python:

```sh
# All four of these are equivalent
objectify use 3fa8 add '{"title": "write tests"}'
objectify use 3fa8 add --parameter '{"title": "write tests"}'
objectify use 3fa8 add -p:title "write tests"
objectify use 3fa8 add --parameter:title "write tests"

# Multiple key-value pairs
objectify use 3fa8 add -p:title "write tests" -p:priority 1

# No input
objectify use 3fa8 pending
```

---

## Schema validation

When you `objectify create --class=TaskList`, objectify runs a one-time schema extraction and stores the resulting JSON Schema in the database. Every subsequent `set` (direct or via method) is validated against it before writing.

**TypeScript:** uses `ts-json-schema-generator` (fetched by Deno automatically) to derive the schema from the `DoBase<T>` generic parameter.

**Python:** walks `__orig_bases__` to find `DoBase[StateType]`, then calls `model_json_schema()` for Pydantic v2, `schema()` for Pydantic v1, or `TypeAdapter(StateType).json_schema()` for dataclasses. Schema extraction is best-effort and skipped silently if Pydantic is not installed.

If the class file changes (detected by mtime), the schema is re-extracted on next use.

---

## Class permissions

**TypeScript classes** run inside Deno's permission sandbox. By default only the classes directory and Deno cache are readable. To grant additional permissions, place a `class.json` sidecar file next to the `.ts` file:

```json
{
  "net": ["api.openai.com"],
  "env": ["OPENAI_API_KEY"],
  "read": ["$HOME/documents"],
  "write": ["$TMPDIR/scratch"]
}
```

All fields are optional. Absent = denied. `true` grants the Deno wildcard for that permission.

| Field | Type | Effect |
|-------|------|--------|
| `net` | `true` \| `["host", ...]` | Network access |
| `read` | `true` \| `["/path", ...]` | Filesystem reads |
| `write` | `true` \| `["/path", ...]` | Filesystem writes |
| `env` | `true` \| `["VAR", ...]` | Environment variables |
| `run` | `true` \| `["binary", ...]` | Subprocess execution |
| `sys` | `["osRelease", ...]` | System info |

**Python classes** run as a normal Python subprocess with no sandboxing. `class.json` is ignored for Python classes.

### Path tokens

| Token | Expands to |
|-------|-----------|
| `$HOME` | User's home directory |
| `$CWD` | Directory where objectify was invoked |
| `$OBJECTIFY_DIR` | Active `.objectify/` directory |
| `$TMPDIR` | System temp directory |

---

## Class introspection

You can inspect the methods available on any class:

```sh
objectify classes TaskList
# Class: TaskList (TypeScript)
#
#   METHOD    PARAMETERS  RETURNS     ASYNC
#   add       { title }   -           yes
#   complete  { id }      -           yes
#   pending   ()          -           yes
#   done      ()          -           yes
#   clear     ()          -           yes
```

Or from an instance directly:

```sh
objectify use 3fa8 help
```

Both produce the same output. JSON output is available via `--json` or by piping.

---

## Shell aliases

Because `objectify create` returns the new object's short ID, you can capture it directly into a shell alias:

```sh
alias task-list="objectify use $(objectify create 'sprint tasks' --class=TaskList)"
```

Now `task-list` behaves like its own command:

```sh
task-list add -p:title "write tests"
task-list add -p:title "fix bug"
task-list pending
task-list complete -p:id "9a3f..."
task-list done
task-list help
```

For persistence across shell sessions, save the alias with a hardcoded ID in your `.zshrc` or `.bashrc`:

```sh
# objectify create "sprint tasks" --class=TaskList → "3fa8"
alias task-list="objectify use 3fa8"
```

Or use a shell function:

```sh
task-list() { objectify use 3fa8 "$@"; }
```

This pattern turns any objectify class into a standalone CLI tool with typed state, versioning, and history — all for free.

---

## Designed for agents

objectify exists because LLM agents need durable, inspectable state — and the file system is a poor fit. Agents write JSON blobs to files, overwrite them without history, lose track of what changed, and have no way to roll back. objectify gives agents the same thing a database gives a web app, but through the interface agents already speak: shell commands with JSON output.

### Why agents need this

Every agentic workflow eventually hits the same wall: the agent needs to remember things across steps, coordinate state between tools, or recover from mistakes. The typical solutions — environment variables, temp files, in-context memory — all break down:

- **Environment variables** disappear when the process ends
- **Temp files** have no schema, no history, and collide across concurrent runs
- **In-context memory** burns tokens and gets evicted when the window fills
- **Databases** require connection strings, drivers, and SQL — overhead that doesn't belong in a tool-calling loop

objectify solves this by giving agents a single binary that speaks JSON over stdin/stdout, with versioned state, schema validation, and method dispatch built in. An agent doesn't need to know SQL or manage connections. It just calls:

```sh
objectify create "session state" --class=AgentMemory
objectify use a3f1 store -p:content "user prefers dark mode" -p:tags '["prefs"]'
objectify use a3f1 recall -p:topK 3
```

### Agent usage patterns

#### Checkpoint before risky operations

```sh
CHECKPOINT=$(objectify fork <id>)   # save state
# ... agent does multi-step work ...
# if it goes wrong:
objectify rewind <id> <version>
```

#### Typed agent memory

```sh
objectify create "session memory" --class=Memory
# → a3f1

objectify use a3f1 store -p:content "the user prefers dark mode" -p:tags '["prefs"]'
objectify use a3f1 recall -p:topK 3
```

#### Shared state across agent steps

```sh
# Step 1: create and populate
objectify create "pipeline state" --class=Pipeline
# → b2c9
objectify use b2c9 set '{"stage": "fetch", "items": []}'

# Step N: read in any subsequent step
objectify use b2c9 get
```

#### Inspect history after a run

```sh
objectify log b2c9
objectify diff b2c9 1 5      # what changed across 5 steps
```

#### Clean up expired objects

```sh
# Create with TTL
objectify create "session context" --class=Context --expire=2h
# → f3c1

# Later: bulk-delete all expired
objectify gc
```

#### Self-describing tools

An agent can discover what it can do with an object without reading source code:

```sh
objectify use a3f1 help
# Returns method names, parameters, and types as JSON
```

This makes objectify classes self-documenting tools that an LLM can reason about at runtime.

### The agent integration model

objectify is designed to be the state layer in a tool-calling agent loop. The typical architecture looks like this:

```
┌──────────────────────────────────────────────────────┐
│                    Agent (LLM)                        │
│                                                       │
│  "I need to track tasks for this sprint"             │
│  → tool_call: bash("objectify create ... --class=TaskList") │
│  → tool_call: bash("objectify use 3fa8 add ...")     │
│  → tool_call: bash("objectify use 3fa8 pending")     │
│                                                       │
│  All tool calls are bash commands.                   │
│  All responses are JSON on stdout.                   │
│  The agent needs no special SDK or driver.           │
└──────────────────────────────────────────────────────┘
         │                           ▲
         │ bash commands             │ JSON stdout
         ▼                           │
┌──────────────────────────────────────────────────────┐
│              objectify (single binary)                │
│                                                       │
│  SQLite (embedded)    Schema validation               │
│  Version history      Fork / rewind                   │
│  Class methods        TypeScript or Python            │
│  Expiry / GC          RFC 6902 diffs                  │
└──────────────────────────────────────────────────────┘
```

Key properties that make this work for agents:

1. **No setup beyond `objectify init`** — no connection strings, no migrations, no config files
2. **JSON in, JSON out** — the format LLMs already think in
3. **Errors on stderr with exit code 1** — agents can detect and react to failures
4. **Schema validation** — prevents agents from writing malformed state that corrupts downstream steps
5. **Append-only history** — every write is recoverable; an agent can never permanently lose state
6. **Class methods as tool definitions** — `objectify use <id> help` returns a machine-readable description of available operations, so an agent can discover capabilities at runtime
7. **Expiry and GC** — agents can create throwaway state without manual cleanup

---

## Comparison with Cloudflare Durable Objects

objectify borrows its core mental model from [Cloudflare Durable Objects](https://developers.cloudflare.com/durable-objects/), but adapts it for a completely different environment: local-first, single-machine, agent-oriented.

### What they share

Both objectify and Cloudflare Durable Objects answer the same fundamental question: *how do you give code a named, persistent, stateful thing to talk to?*

| Concept | Cloudflare Durable Objects | objectify |
|---------|---------------------------|-----------|
| Core abstraction | A JavaScript class with persistent state | A TypeScript/Python class with persistent state |
| Identity | Global unique ID | Random hex with prefix resolution |
| State access | `this.state.get()` / `this.state.put()` | `this.get()` / `this.set()` |
| Behavior | Methods on the class handle requests | Methods on the class handle commands |
| Consistency | Single-threaded per object (strong consistency) | Single-process per object (SQLite serialization) |
| Lifecycle | Created on first access, garbage collected | Created explicitly, expired via TTL + `gc` |

The programming model is intentionally similar. If you've written a Durable Object, writing an objectify class will feel familiar:

**Cloudflare Durable Object:**

```ts
export class Counter {
  constructor(state, env) { this.state = state; }
  async fetch(request) {
    let val = (await this.state.get("count")) || 0;
    val++;
    await this.state.put("count", val);
    return new Response(val.toString());
  }
}
```

**objectify class:**

```ts
export default class Counter extends DoBase<{ count: number }> {
  increment = async ({ by = 1 }: { by?: number } = {}) => {
    const { count = 0 } = (await this.get()) || {};
    await this.set({ count: count + by });
    return count + by;
  };
}
```

### Where they diverge

The differences stem from their target environments. Cloudflare Durable Objects run on a global edge network serving HTTP requests at scale. objectify runs on a single machine serving an LLM agent via bash.

| | Cloudflare Durable Objects | objectify |
|---|---|---|
| **Environment** | Cloudflare's edge network (V8 isolates) | Local machine (single binary + SQLite) |
| **Interface** | HTTP `fetch()` requests | Shell commands with JSON I/O |
| **Consumer** | Web applications, APIs | LLM agents, shell scripts, CLI tools |
| **Networking** | Built-in (WebSocket, HTTP) | None (local only; classes can make network calls) |
| **Distribution** | Globally distributed, single-point-of-consistency | Single machine, single SQLite file |
| **State storage** | Cloudflare's distributed KV (key-value pairs) | SQLite (full JSON snapshots) |
| **Versioning** | No built-in history | Every write creates a version; full history with rewind and fork |
| **Schema** | No built-in validation | JSON Schema extracted from types, validated on every write |
| **Languages** | JavaScript only | TypeScript (Deno, sandboxed) and Python (unsandboxed) |
| **Billing model** | Pay per request, duration, and storage | Free (it's a local binary) |
| **Deployment** | `wrangler deploy` to Cloudflare | `objectify init` in any directory |

### When to use which

**Use Cloudflare Durable Objects when:**
- You need globally distributed state with strong consistency
- You're building a web application or API that handles concurrent HTTP requests
- You need WebSocket coordination (chat rooms, collaborative editing, game servers)
- You want Cloudflare's infrastructure to handle scaling, replication, and fault tolerance

**Use objectify when:**
- You need persistent, typed state for an LLM agent workflow
- You want version history, rewind, and fork on every piece of state
- You're working locally and don't need network distribution
- You want shell-native JSON I/O that any tool-calling agent can use without an SDK
- You need schema validation to prevent agents from writing malformed state
- You want to inspect, diff, and audit what an agent did after a run

### The philosophical difference

Cloudflare Durable Objects are infrastructure for distributed systems. objectify is a *tool* for agents. Cloudflare's design optimizes for scale, latency, and global consistency. objectify's design optimizes for inspectability, recoverability, and ease of integration with LLM tool-calling loops.

The version history is the clearest example of this divergence. Cloudflare doesn't keep history because web apps rarely need to rewind state — they need fast reads and writes at scale. objectify keeps *every* version because agents make mistakes, and the ability to inspect what happened and roll back is more valuable than raw performance.

Similarly, objectify's JSON-over-stdout contract exists specifically because that's how LLM agents consume tools. An agent calling `objectify use 3fa8 pending` gets back a JSON array it can reason about directly. No HTTP client, no request/response parsing, no authentication — just a bash command and a JSON result.

---

## Comparison with MCP (Model Context Protocol)

[MCP](https://modelcontextprotocol.io/) is Anthropic's open protocol for connecting LLM agents to external tools and data sources. It defines a client-server architecture where MCP servers expose tools, resources, and prompts over JSON-RPC (via stdio or HTTP/SSE), and MCP clients (embedded in agent hosts like Claude Desktop, VS Code, or custom apps) call them.

objectify solves a overlapping problem — giving agents typed, callable tools — but takes a fundamentally different approach. Understanding the tradeoffs helps you decide when to use each, or both together.

### What MCP gives you

An MCP server is a process that exposes a set of tools an agent can call. Here's a minimal MCP server that manages a counter:

```ts
// counter-mcp-server.ts
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

let count = 0;

const server = new McpServer({ name: "counter", version: "1.0.0" });

server.tool("increment", { by: z.number().optional() }, async ({ by = 1 }) => {
  count += by;
  return { content: [{ type: "text", text: JSON.stringify(count) }] };
});

server.tool("get", {}, async () => {
  return { content: [{ type: "text", text: JSON.stringify(count) }] };
});

server.tool("reset", {}, async () => {
  count = 0;
  return { content: [{ type: "text", text: "reset" }] };
});

const transport = new StdioServerTransport();
await server.connect(transport);
```

To use this, you need to:

1. Install the MCP SDK (`npm install @modelcontextprotocol/sdk`)
2. Write and maintain the server code above
3. Configure your agent host to connect to it (JSON config file pointing to the server binary)
4. The agent host must support MCP (Claude Desktop, Cursor, Claude Code, etc.)
5. State lives in the server process — if it crashes, `count` is gone

### What objectify gives you

The equivalent objectify class:

```ts
// .objectify/classes/Counter.ts
import { DoBase } from 'objectify';

export default class Counter extends DoBase<{ count: number }> {
  increment = async ({ by = 1 }: { by?: number } = {}) => {
    const { count = 0 } = (await this.get()) || {};
    await this.set({ count: count + by });
    return count + by;
  };

  reset = async () => {
    await this.set({ count: 0 });
  };
}
```

To use this, you:

1. Drop the file in `.objectify/classes/`
2. That's it

Any agent that can call bash can use it immediately:

```sh
objectify create "my counter" --class=Counter
# → 3fa8
objectify use 3fa8 increment -p:by 5
# → 5
objectify use 3fa8 help
# → list of methods with parameters
```

No SDK, no server process, no client configuration, no protocol negotiation. And the state is automatically persisted, versioned, and recoverable.

### Side-by-side comparison

| | MCP | objectify |
|---|---|---|
| **What it is** | Protocol for agent-tool communication | CLI for persistent stateful objects |
| **Architecture** | Client-server (JSON-RPC over stdio/HTTP) | Single binary (bash commands, JSON stdout) |
| **Tool definition** | Server code + SDK + transport setup | Drop a `.ts` or `.py` file in a directory |
| **Agent requirements** | Agent host must implement MCP client | Agent must be able to call bash |
| **State** | You manage it (in-memory, database, files) | Built-in (SQLite, versioned, schema-validated) |
| **State history** | You build it yourself | Every write creates a version automatically |
| **Rollback** | You build it yourself | `objectify rewind <id> <version>` |
| **Schema validation** | You add it (Zod, JSON Schema, etc.) | Extracted from types automatically, validated on every write |
| **Tool discovery** | `tools/list` protocol method | `objectify use <id> help` or `objectify classes <name>` |
| **Multiple instances** | Run separate servers or add routing | `objectify create` — each object is independent |
| **Resources & prompts** | First-class MCP concepts | Not applicable (objectify is tools + state only) |
| **External services** | Community MCP servers for popular APIs | Full access to npm/pip ecosystems — write a class that calls any API |
| **Ecosystem** | Growing marketplace of pre-built MCP servers | Write your own classes (simpler, but no marketplace) |
| **Setup cost** | Moderate (SDK, config, server process) | Minimal (`objectify init`, drop a file) |

### The key differences

**1. State is the core difference.**

MCP is stateless by design. The protocol defines how to *call* tools, but says nothing about how those tools store data. If your MCP server needs to remember something between calls, you're back to managing a database, file system, or in-memory store yourself — with no versioning, no schema validation, and no rollback.

objectify makes state a first-class primitive. Every object has a typed JSON state, every write creates a version, and every version is recoverable. You don't build this — you get it by default.

**2. Transport vs. interface.**

MCP defines a transport protocol (JSON-RPC with capabilities negotiation, tool listing, structured content types). This is powerful for building an ecosystem of interoperable tools, but it means every tool needs a server process and every agent needs an MCP client.

objectify uses bash as the transport. There's nothing to negotiate, no handshake, no capabilities exchange. The agent runs a command and reads JSON from stdout. This works in every agent framework, every shell script, every CI pipeline, and every environment where bash exists — which is everywhere.

**3. Instances vs. singletons.**

An MCP server typically exposes a fixed set of tools. If you want two independent counters, you need to add routing logic to your server (or run two server processes). MCP has no concept of instances.

objectify's model is inherently instance-based. `objectify create --class=Counter` gives you a new, independent counter every time. You can have hundreds of Counter objects, each with its own state and history, managed by the same class definition.

**4. Lifecycle and discovery.**

MCP servers are long-running processes that need to be started, configured, and connected. If the server dies, the tools are gone. Discovery happens through the MCP protocol's `tools/list` method, which requires an active connection.

objectify classes are files on disk. They're discovered by filename, available instantly, and never crash (because there's no server to crash — the runtime is only invoked for the duration of a single method call). Discovery works offline: `objectify classes` lists everything available.

### What about external services?

A common assumption is that MCP is needed for external API integration. But objectify classes are full TypeScript or Python programs — they have access to the entire npm and pip ecosystems. A Python class can `import requests` and call any REST API. A TypeScript class can `import { Octokit } from "npm:octokit"` and talk to GitHub. There's no sandbox restriction that prevents network access (Python classes are unsandboxed; TypeScript classes can be granted network permissions via `class.json`).

```ts
// .objectify/classes/GitHubIssues.ts — calls the GitHub API directly
import { DoBase } from 'objectify';
import { Octokit } from 'npm:octokit';

export default class GitHubIssues extends DoBase<{ issues: any[] }> {
  sync = async ({ repo }: { repo: string }) => {
    const octokit = new Octokit();
    const { data } = await octokit.rest.issues.list({
      owner: repo.split('/')[0],
      repo: repo.split('/')[1],
    });
    await this.set({ issues: data });
    return { synced: data.length };
  };
}
```

The real advantage MCP has over objectify for external services is not capability — it's **packaging**. Someone has already written an MCP server for Slack, GitHub, Postgres, Stripe, and dozens of other services. You configure it and it works without writing any code. With objectify, you write the integration class yourself.

But for most agent workflows, writing a 30-line class that calls an API and stores the result beats installing and configuring an MCP server. The MCP ecosystem advantage only matters when someone has already built exactly the integration you need and you don't want to write it yourself. And even then, the MCP server won't give you versioned state, rollback, or audit history — you'd still need something like objectify for that.

### When to use MCP

- You want to use **pre-built MCP servers** from the ecosystem without writing integration code
- Your agent host **already supports MCP** and you want plug-and-play tool integration
- You need MCP-specific features: **resources** (structured data the agent can browse), **prompts** (reusable prompt templates), or **sampling** (letting the server ask the LLM for help)
- You're building a **multi-agent system** where tools need to be shared across different agent processes via a standard protocol

### When to use objectify

- Your tools need **persistent, versioned state** — and you don't want to build that infrastructure yourself
- You want **instance-based** tools (multiple independent counters, task lists, pipelines) rather than singletons
- You need **rollback, fork, and diff** on your tool state — for recovery, auditing, or branching workflows
- You want **zero-config tool creation** — drop a file, it works
- Your agent already has **bash access** (which almost all do) and you don't want to set up MCP infrastructure
- You want to **inspect what an agent did** after a run: `objectify log`, `objectify diff`, full version history
- You want to **integrate with external APIs** without the overhead of running a server process — just write the class

### Using them together

MCP and objectify are not mutually exclusive. A natural pattern is to use objectify *inside* an MCP server for state management:

```ts
// An MCP server that uses objectify for persistent state
server.tool("add_task", { title: z.string() }, async ({ title }) => {
  const result = execSync(
    `objectify use ${TASK_LIST_ID} add -p:title "${title}"`
  ).toString();
  return { content: [{ type: "text", text: result }] };
});
```

Or use objectify as the "state layer" and MCP as the "integration layer": MCP servers connect to external APIs, while objectify manages the agent's internal state, memory, and workflow progress.

You can also wrap an entire objectify class as an MCP server — each method becomes a tool, and objectify handles all the state management behind the scenes. The MCP server becomes a thin adapter between the protocol and objectify's CLI.

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
