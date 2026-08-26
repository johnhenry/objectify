# Examples

Runnable, self-checking examples. Each one is named for the **behavior it
demonstrates**, drives a real store in a temp directory (`mktemp -d`, cleaned
up on exit), and asserts its own output — if it prints `OK: ...` and exits 0,
the behavior holds.

## Shell examples (CLI)

| Example | Demonstrates |
| --- | --- |
| [`01-state-survives-across-processes.sh`](01-state-survives-across-processes.sh) | State written by one CLI invocation is readable by a later, unrelated process — persistence with no server. |
| [`02-every-write-is-a-version-you-can-read-back.sh`](02-every-write-is-a-version-you-can-read-back.sh) | `set` appends versions; `log` lists them, `get --at=N` time-travels, `diff` emits an RFC 6902 JSON Patch. |
| [`03-rewind-restores-without-deleting-history.sh`](03-rewind-restores-without-deleting-history.sh) | `rewind` restores old state as a *new* version — the mistake and the rewind both stay in the log. |
| [`04-forks-evolve-independently.sh`](04-forks-evolve-independently.sh) | `fork` copies an object into a new ID; mutations to either side never affect the other. |
| [`05-expired-objects-linger-until-gc.sh`](05-expired-objects-linger-until-gc.sh) | Expiry is soft: expired objects still answer until `gc` reaps them — and `gc` touches nothing else. |

### Running

Each script finds the CLI via the shared helper [`_lib.sh`](_lib.sh), which
supports two methods:

1. **Prebuilt binary** (fast): if `target/release/objectify` exists, it is used
   directly. Build it once with:

   ```sh
   cargo build --release
   ```

2. **Fallback via cargo** (no prior build needed): otherwise the helper runs
   `cargo run --release --quiet -- <args>` for every CLI call. Slower on the
   first call (it compiles), identical behavior after that.

Then:

```sh
./examples/01-state-survives-across-processes.sh   # run one
for f in examples/0*.sh; do "$f"; done             # run them all
```

Examples never touch your real `~/.objectify` or the repo's store: each one
`objectify init`s inside its own temp directory and removes it via a `trap` on
exit.

## Node examples (objectify-js adapter)

| Example | Demonstrates |
| --- | --- |
| [`06-js-adapter-and-cli-share-the-same-store.mjs`](06-js-adapter-and-cli-share-the-same-store.mjs) | A Node process and the Rust CLI read and write the same object interchangeably — one SQLite store, two clients. |
| [`07-versioning-works-the-same-from-javascript.mjs`](07-versioning-works-the-same-from-javascript.mjs) | The full lifecycle — set, time-travel `get(v)`, `diff`, `rewind`, `fork`, `gc` — entirely in-process from JS. |

> **Requires PR #3 (`fix/objectify-js-npm-scope-and-tests`) to be merged
> first.** On current Node (22+/26), `packages/objectify-js` as it exists on
> `main` does **not** work: its TypeScript compiles, but the pinned
> `better-sqlite3 ^11` has no prebuilt binary for modern Node ABIs and fails to
> compile from source, so the adapter cannot load. PR #3 bumps it to `^13`,
> which fixes this. The examples are written against the adapter's public API
> (`Objectify` / `ObjectRef`), which is identical on `main` and on PR #3's
> branch — they were verified green against `better-sqlite3@13`.

Once PR #3 is merged:

```sh
cd packages/objectify-js
npm install && npm run build     # produces dist/
cd ../..
node examples/06-js-adapter-and-cli-share-the-same-store.mjs
node examples/07-versioning-works-the-same-from-javascript.mjs
```

Example 06 also invokes the CLI and locates it the same two ways as the shell
examples (release binary first, `cargo run` fallback).

## CI

These examples are **not** wired into CI: the repository currently has no
GitHub Actions workflows at all (no `.github/workflows/`), and adding a
Rust build pipeline is out of scope for an examples change. If a cargo-based
workflow is added later, a smoke step like
`for f in examples/0*.sh; do "$f"; done` after `cargo build --release` would
cover them.
