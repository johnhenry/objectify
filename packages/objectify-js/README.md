# @johnhenry/objectify-js

TypeScript adapter for [objectify](../../README.md) — persistent, versioned JSON objects backed by SQLite.

Talks directly to the same SQLite database as the `objectify` CLI. No shelling out, no server process. Use it to give your Node.js apps versioned, typed state — or as the state layer behind an RPC framework like [oRPC](https://orpc.dev/).

> **Provenance:** this package was previously an internal, unpublished
> package (`objectify-js@0.1.0`, workspace-local only — it never had a
> version on the npm registry). It is being published for the first time
> as `@johnhenry/objectify-js`, restarting at `0.0.0` per the
> [`@johnhenry` adoption convention](https://opensource.johnhenry.me/).

## Installation

```sh
npm install @johnhenry/objectify-js
```

Requires Node.js 22+ (matches [`better-sqlite3`](https://github.com/WiseLibs/better-sqlite3)'s minimum supported version — its N-API prebuilds are what let this package install without a native compiler toolchain).

Requires an initialized objectify store. If you haven't already:

```sh
# install the CLI
cargo install --path .

# create a store
objectify init
```

## Quick start

```ts
import { Objectify } from '@johnhenry/objectify-js';

const store = new Objectify(); // auto-finds .objectify/ walking up from cwd

// Create an object
const id = store.create({ description: 'my config' });

// Get a handle to it
const ref = store.use(id);

// Read and write state
ref.set({ theme: 'dark', fontSize: 14 });
ref.get(); // → { theme: 'dark', fontSize: 14 }

// Every write is versioned
ref.set({ theme: 'light', fontSize: 14 });
ref.log();
// → [
//   { version: 1, method: 'create', at: '...' },
//   { version: 2, method: 'set', at: '...' },
//   { version: 3, method: 'set', at: '...' },
// ]

// Rewind to any version
ref.rewind(2); // non-destructive, creates version 4

// Diff between versions
ref.diff(2, 3); // → RFC 6902 JSON Patch

// Fork into an independent copy
const forkId = ref.fork();

store.close();
```

## Class methods

If the object has a class (a `.ts` or `.py` file in `.objectify/classes/`), you can call its methods:

```ts
const id = store.create({ description: 'sprint tasks', class: 'TaskList' });
const tasks = store.use(id);

tasks.call('add', { title: 'write tests' });
// → { id: '...', title: 'write tests', done: false, createdAt: '...' }

tasks.call('pending');
// → [{ id: '...', title: 'write tests', done: false }]

tasks.call('complete', { id: '...' });
tasks.call('done');
// → [{ id: '...', title: 'write tests', done: true }]
```

Class methods execute via Deno (TypeScript) or Python 3 — the same runtimes the CLI uses. State is validated against the class schema on every write.

## API reference

### `Objectify`

```ts
const store = new Objectify(opts?)
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `dir` | `string` | auto-detect | Path to `.objectify/` directory |

**Methods:**

| Method | Returns | Description |
|--------|---------|-------------|
| `create(opts?)` | `string` | Create an object, returns short ID |
| `destroy(idPrefix)` | `void` | Permanently delete an object |
| `use(idPrefix)` | `ObjectRef` | Get a handle to an object |
| `inspect(idPrefix)` | `InspectResult` | Object metadata |
| `list(opts?)` | `InspectResult[]` | List objects with optional filters |
| `gc()` | `number` | Delete expired objects, returns count |
| `close()` | `void` | Close the database connection |

**`create` options:**

| Option | Type | Description |
|--------|------|-------------|
| `description` | `string` | Human-readable label |
| `class` | `string` | Class name (must exist as `.ts` or `.py` in `classes/`) |
| `expire` | `string` | TTL duration (`30s`, `15m`, `2h`, `7d`, `2w`) |

**`list` options:**

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `class` | `string` | — | Filter by class name |
| `expired` | `boolean` | `false` | Include expired objects |
| `since` | `string` | — | ISO date or relative duration |
| `limit` | `number` | `50` | Max results |
| `offset` | `number` | `0` | Pagination offset |

### `ObjectRef`

Returned by `store.use(idPrefix)`. All operations are scoped to a single resolved object.

| Method | Returns | Description |
|--------|---------|-------------|
| `get(version?)` | `T` | Current state, or state at a specific version |
| `set(state)` | `void` | Replace state (validates against schema) |
| `call(method, input?)` | `unknown` | Call a class method |
| `log()` | `LogEntry[]` | Version history |
| `diff(v1, v2)` | `Operation[]` | RFC 6902 JSON Patch between two versions |
| `rewind(version)` | `{ rewoundTo, newVersion }` | Restore to a previous version |
| `fork(opts?)` | `string` | Create an independent copy, returns short ID |
| `inspect()` | `InspectResult` | Object metadata |

**Properties:**

| Property | Type | Description |
|----------|------|-------------|
| `id` | `string` | Full 32-character hex ID |
| `shortId` | `string` | Minimum unique prefix |

---

## Using with oRPC

objectify-js is a natural fit as the state layer behind [oRPC](https://orpc.dev/) procedures. oRPC handles the network boundary (HTTP, type safety, OpenAPI spec generation), while objectify handles the state boundary (persistence, versioning, schema validation). Together, you get typed RPC endpoints with built-in version history, rollback, and audit trails — without writing any state management code.

### Basic setup

```ts
import { Objectify } from '@johnhenry/objectify-js';
import { os } from '@orpc/server';
import { z } from 'zod';

// One store, shared across all routes
const store = new Objectify();
```

### A task list API

Assume you have a `TaskList` class in `.objectify/classes/TaskList.ts` with methods `add`, `complete`, `pending`, and `done`.

```ts
// Create the object once (or look up an existing one)
const TASKS_ID = store.create({ description: 'sprint tasks', class: 'TaskList' });
const tasks = store.use(TASKS_ID);

const taskRouter = {
  // Each class method becomes an oRPC procedure
  add: os
    .input(z.object({ title: z.string() }))
    .handler(({ input }) => tasks.call('add', input)),

  complete: os
    .input(z.object({ id: z.string() }))
    .handler(({ input }) => tasks.call('complete', input)),

  pending: os
    .handler(() => tasks.call('pending')),

  done: os
    .handler(() => tasks.call('done')),

  // Version history and rollback come free from objectify
  history: os
    .handler(() => tasks.log()),

  diff: os
    .input(z.object({ v1: z.number(), v2: z.number() }))
    .handler(({ input }) => tasks.diff(input.v1, input.v2)),

  rewind: os
    .input(z.object({ version: z.number() }))
    .handler(({ input }) => tasks.rewind(input.version)),
};
```

That's it. Every procedure gets automatic OpenAPI docs via oRPC, and every state mutation gets automatic versioning via objectify. The client gets type-safe calls:

```ts
const tasks = await orpc.tasks.pending();
const task = await orpc.tasks.add({ title: 'write tests' });
await orpc.tasks.complete({ id: task.id });

// Audit and recover
const history = await orpc.tasks.history();
const changes = await orpc.tasks.diff({ v1: 1, v2: 5 });
await orpc.tasks.rewind({ version: 3 });
```

### Multiple instances

objectify's instance model means each `create` produces an independent object. This maps naturally to multi-tenant or per-resource APIs:

```ts
const projectRouter = {
  create: os
    .input(z.object({ name: z.string() }))
    .handler(({ input }) => {
      const id = store.create({
        description: input.name,
        class: 'ProjectBoard',
      });
      return { id };
    }),

  use: os
    .input(z.object({ id: z.string(), method: z.string(), input: z.any().optional() }))
    .handler(({ input }) => {
      const ref = store.use(input.id);
      return ref.call(input.method, input.input);
    }),
};
```

Each project gets its own versioned state, its own history, and its own rollback capabilities — with no additional infrastructure.

### What this gives you that oRPC alone doesn't

| Concern | oRPC alone | oRPC + objectify |
|---------|-----------|-----------------|
| Type-safe client calls | Yes | Yes |
| OpenAPI spec | Yes | Yes |
| Persistent state | You build it | Built-in |
| Version history | You build it | `tasks.log()` |
| Rollback | You build it | `tasks.rewind(n)` |
| Audit diff | You build it | `tasks.diff(a, b)` |
| Fork state | You build it | `tasks.fork()` |
| Schema validation | Zod at the edge | Zod + JSON Schema on state |
| Multiple instances | You add routing | `store.create()` |

The objectify class defines the business logic once. oRPC exposes it over HTTP. Neither needs to know about the other's internals.

---

## Shared database with the CLI

The adapter reads and writes the same `objectify.db` file as the `objectify` CLI. This means you can:

- Create objects from the CLI, use them from Node.js (or vice versa)
- Inspect state with `objectify log` / `objectify diff` while your server is running
- Use shell aliases alongside your API: `alias tasks="objectify use 3fa8"`

SQLite WAL mode ensures safe concurrent reads between the CLI and the adapter. Writes are serialized by SQLite's locking — this is fine for typical usage but not designed for high-concurrency write workloads.

## License

MIT
