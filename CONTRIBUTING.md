# Contributing

## Prerequisites

- [Rust](https://rustup.rs) 1.70 or later
- [Deno](https://deno.land) (optional — needed only to run class method tests)

## Building

```sh
git clone <repo>
cd objectify
cargo build          # debug
cargo build --release  # optimized, stripped binary
```

The binary lands at `target/release/objectify` (or `target/debug/objectify` for debug builds). SQLite is compiled and linked statically — no system dependencies required.

## Project structure

```
src/
  main.rs       — everything: CLI, context resolution, DB, IDs,
                  time utilities, class runner, all command implementations
Cargo.toml
Cargo.lock
```

The codebase is intentionally kept in a single file. Resist splitting it unless a module becomes independently testable and reusable.

## Running tests

```sh
cargo test
```

For integration tests that exercise class method execution, Deno must be on PATH.

## Key design constraints to preserve

**IDs are ULIDs with prefix resolution.** Do not switch to sequential integers or UUIDs. Prefix resolution (any unambiguous prefix resolves to the full ID) is load-bearing for ergonomics.

**stdout is always valid JSON, stderr is always `{"error": "..."}`.** Every code path must honor this. Table output for human-readable commands (`list`, `log`) is only rendered when stdout is a TTY — piping always gets JSON.

**`set` is always full replacement.** Never add merge semantics. Agents rely on explicit read-then-write for predictability.

**History is append-only.** `rewind` writes a new version, never deletes. `destroy` and `gc` are the only deletions, and they are explicit and permanent.

**Full snapshots, no deltas.** Every event row stores the complete JSON state. This keeps the schema simple and history directly readable without reconstruction.

**Deno is optional.** The commands `get`, `set`, `list`, `log`, `diff`, `rewind`, `fork`, and `gc` must never require Deno. Only `use <id> <method>` (class method dispatch) touches Deno.

**No background processes.** No daemon, no cron, no pid files. GC is lazy and triggered only by `objectify gc`.

## Adding a command

1. Add a variant to the `Command` enum in `main.rs`.
2. Write a `cmd_<name>` function that returns `Result<()>`.
3. Add a match arm in `run()`.
4. Document it in `README.md` and add a line to `CHANGELOG.md`.

## Making a release

1. Update the version in `Cargo.toml`.
2. Add a section to `CHANGELOG.md`.
3. Tag: `git tag v0.x.y && git push --tags`
4. CI builds release binaries for `x86_64-linux`, `aarch64-linux`, `x86_64-darwin`, `aarch64-darwin` and attaches them to the GitHub release.

## Reporting issues

Open a GitHub issue. Include the objectify version (`objectify --version`), the command that failed, and the full stderr output.
