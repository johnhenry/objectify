# objectify

Persistent, versioned JSON objects with optional typed behavior, designed for LLM agent use.

---

## IDs

Objects are assigned a full random ID at creation. Any unique prefix resolves to that object — same model as git. The minimum unambiguous prefix is shown in all output.

```sh
objectify create "my thing"
# → 3fa8

objectify use 3fa8 get     # works
objectify use 3f get       # works if unique
objectify use 3 get        # error if ambiguous: matches 3fa8, 3c21...
```

---

## Directory resolution

`objectify` looks for `.objectify/` in the current directory first, then `~/.objectify/`. Local always wins. Classes defined in either location are available to all objects in that scope.

```
.objectify/          (or ~/.objectify/)
  classes/
    TaskList.ts
    Memory.ts
  deno.json
  deno.lock
  objectify.db
```

---

## Commands

### Init

```sh
objectify init             # creates .objectify/ in cwd
objectify init --global    # creates ~/.objectify/
```

Creates the directory structure, a blank `deno.json`, and initializes the SQLite db.

---

### Create

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

Expiry supports: `s`, `m`, `h`, `d`, `w` (e.g. `30m`, `7d`, `2w`).

---

### Destroy

```sh
objectify destroy <id>
```

Deletes the object and all its history permanently.

---

### Inspect

```sh
objectify inspect <id>
```

Prints object metadata: full ID, class, description, version count, created at, expires at.

```json
{
  "id": "3fa8c291d6b0",
  "class": "TaskList",
  "description": "sprint tasks",
  "versions": 4,
  "createdAt": "2025-03-10T14:22:00Z",
  "expiresAt": null
}
```

---

### List

```sh
objectify list [options]

Options:
  --class=ClassName     filter by class
  --expired             include garbage collected objects
  --since=<date>        created on or after date (ISO or relative: 2d, 1w)
  --limit=N             default 50
  --offset=N            for pagination
  --json                machine-readable output
```

Default output is a table:

```
ID      CLASS       DESCRIPTION      VER  CREATED        EXPIRES
3fa8    TaskList    sprint tasks     4    2 hours ago    never
cc01    -           temp flags       1    5 mins ago     in 7 days
a1b2    Memory      project memory   12   3 days ago     never
```

With `--json`:
```json
[
  {
    "id": "3fa8c291d6b0",
    "shortId": "3fa8",
    "class": "TaskList",
    "description": "sprint tasks",
    "versions": 4,
    "createdAt": "2025-03-10T14:22:00Z",
    "expiresAt": null
  }
]
```

---

### Use

#### get

```sh
objectify use <id> get [--at=<version>]
```

Returns the current state as JSON. `--at` accepts a version number.

```sh
objectify use 3fa8 get
# → {"tasks": [{"id": "9a3f", "title": "write tests", "done": false}]}

objectify use 3fa8 get --at=1
# → {"tasks": []}
```

#### set

```sh
objectify use <id> set <json>
```

Full replacement. Validates against class schema if present. Writes a new version on success.

```sh
objectify use a1b2 set '{"count": 42}'
```

Errors go to stderr, exit 1. No version is written on failure.

#### custom method

```sh
objectify use <id> <method> [json-input]
```

Calls a method defined on the object's class. Input is optional depending on method signature.

```sh
objectify use 3fa8 add '{"title": "write tests"}'
# → {"id": "9a3f", "title": "write tests", "done": false, "createdAt": "..."}

objectify use 3fa8 pending
# → [{"id": "9a3f", "title": "write tests", "done": false}]
```

---

### History

#### log

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

#### diff

```sh
objectify diff <id> <v1> <v2>
```

JSON diff between two versions. Output is a standard RFC 6902 JSON Patch array.

#### rewind

```sh
objectify rewind <id> <version>
```

Restores state to a previous version. This is non-destructive — it writes a new version record pointing to the old state. History is never deleted.

```sh
objectify rewind 3fa8 2
# → rewound to version 2, now at version 5
```

#### fork

```sh
objectify fork <id> [--at=<version>]
```

Creates a new independent object with copied state. Returns the new short ID.

```sh
objectify fork 3fa8
# → b9c1

objectify fork 3fa8 --at=2
# → d4e2
```

---

## Output contract

- All stdout is valid JSON (even single values)
- All errors go to stderr as `{"error": "message"}`
- Exit 0 on success, 1 on error
- `--json` flag is available globally but is a no-op on commands that already output JSON

---

## Classes

Drop a `.ts` file in `.objectify/classes/` (or `~/.objectify/classes/`). No registration needed — objectify discovers by filename. Class name must match the default export.

### Base class

```ts
// provided by objectify, imported via deno.json import map
import { DoBase, DoContext } from 'objectify';

export default class MyClass extends DoBase<MyState> {
  myMethod = async (input: MyInput) => {
    const state = await this.get();
    await this.set({ ...state, ...changes });
    return result;
  };
}
```

`this.get()` and `this.set()` are injected at runtime. The generic `T` on `DoBase<T>` is extracted at class load time via `ts-json-schema-generator` and stored as the runtime validation schema for that class.

### Example: TaskList

```ts
// .objectify/classes/TaskList.ts

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
      tasks: tasks.map(t => t.id === id ? { ...t, done: true } : t),
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

### Example: Memory (with npm dep via deno)

```ts
// .objectify/classes/Memory.ts
import { similarity } from 'npm:ml-distance';

interface Entry {
  id: string;
  content: string;
  embedding: number[];
  tags: string[];
  createdAt: string;
}

interface MemoryState {
  entries: Entry[];
}

export default class Memory extends DoBase<MemoryState> {
  store = async ({ content, embedding, tags = [] }: {
    content: string;
    embedding: number[];
    tags?: string[];
  }) => {
    const { entries = [] } = await this.get();
    const entry: Entry = {
      id: crypto.randomUUID(),
      content,
      embedding,
      tags,
      createdAt: new Date().toISOString(),
    };
    await this.set({ entries: [...entries, entry] });
    return entry.id;
  };

  recall = async ({ embedding, topK = 5 }: { embedding: number[]; topK?: number }) => {
    const { entries = [] } = await this.get();
    return entries
      .map(e => ({ ...e, score: similarity.cosine(embedding, e.embedding) }))
      .sort((a, b) => b.score - a.score)
      .slice(0, topK)
      .map(({ embedding: _, ...rest }) => rest);
  };

  forget = async ({ id }: { id: string }) => {
    const { entries } = await this.get();
    await this.set({ entries: entries.filter(e => e.id !== id) });
  };
}
```

---

## SQLite schema

```sql
CREATE TABLE objects (
  id          TEXT PRIMARY KEY,  -- full id
  class       TEXT,
  description TEXT,
  schema      TEXT,              -- JSON schema derived from DoBase<T>, nullable
  created_at  TEXT NOT NULL,
  expires_at  TEXT               -- nullable
);

CREATE TABLE events (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  object_id   TEXT NOT NULL REFERENCES objects(id),
  version     INTEGER NOT NULL,
  method      TEXT NOT NULL,     -- 'create' | 'set' | method name
  state       TEXT NOT NULL,     -- full JSON snapshot
  created_at  TEXT NOT NULL
);

CREATE INDEX idx_events_object ON events(object_id, version);
```

Every write is a full snapshot. No delta storage — history is always directly readable without reconstruction.
