---
title: "CLI reference"
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
