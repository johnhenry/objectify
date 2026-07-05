---
title: "objectify"
---

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
