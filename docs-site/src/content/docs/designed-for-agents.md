---
title: "Designed for agents"
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
