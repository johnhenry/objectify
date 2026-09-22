# Agent playbook

A polyglot repo, not a single-language monorepo: the CLI itself is one Rust
crate (`Cargo.toml` at root, all source in `src/main.rs` by design — see
"Repo-specific gotchas"), built with `cargo`, tested with `cargo test`, and
distributed to npm as prebuilt binaries. Alongside it, `packages/objectify-js`
is a real npm workspace (`workspaces: ["packages/*"]` in root
`package.json`) holding one publishable TypeScript adapter, tested with
`node --test`. `npm/` holds six hand-maintained, publish-only
`package.json` manifests (the `@johnhenry/objectify` shim plus five
platform packages) that are **not** part of the npm workspace — they exist
only to be populated with a built binary and published by `release.yml`,
never built or tested via `npm run build`/`test`.

`CLAUDE.md` in this directory is a symlink to this file.

## Workspace structure and build order

| Component | Language | Role |
| --- | --- | --- |
| [`src/main.rs`](src) (root `Cargo.toml`) | Rust | The CLI itself: parsing, ID management, SQLite, schema validation, JSON diff, class-subprocess dispatch. One crate, one binary. |
| [`packages/objectify-js`](packages/objectify-js) | TypeScript | `@johnhenry/objectify-js` — the TypeScript adapter (`DoBase<T>`, SQLite store). The one real npm workspace member. |
| [`npm/objectify`](npm/objectify) | JS (hand-maintained) | `@johnhenry/objectify` — the published shim: platform-detects at install time and `exec`s the matching platform package's binary. Not built from source in this repo; assembled at release time. |
| [`npm/{darwin,linux,win32}-{arm64,x64}`](npm) | none (binary only) | Five `@johnhenry/objectify-<platform>` packages, each carrying one prebuilt `objectify` binary plus a `checksums.json`. Populated and published only by `release.yml`. |

There is no cross-language build dependency in the usual monorepo sense —
`objectify-js` does not depend on the Rust binary at build time, and the
Rust binary does not depend on any npm package. The two sides are tested by
**fully separate CI workflows**, not one job matrix: `.github/workflows/ci.yml`
(Rust, cross-platform matrix, path-unfiltered) and
`.github/workflows/objectify-js-ci.yml` (Node, path-filtered to
`packages/objectify-js/**`). A change to one language's code alone never
triggers the other's CI job.

## The verification loop (before every push)

Rust and npm are verified independently; run both before a change that
touches either side, and only the relevant one for a change scoped to one
side.

```bash
# Rust (the CLI itself)
cargo build --release --locked
cargo test --release --locked

# npm (objectify-js only -- npm/*'s manifests are never built or tested)
npm run build                                    # turbo run build
npm test --workspace @johnhenry/objectify-js      # node --test dist/objectify.test.js
```

CI matches this split: `ci.yml` runs `cargo build --release --locked` then
`cargo test --release --locked` on `linux-x64`, `linux-arm64`, and
`windows-x64` (macOS is intentionally excluded from per-PR CI — see
gotchas — but is built and tested for real in `release.yml`).
`objectify-js-ci.yml` runs the npm side, gated by `engines.node >=26.0.0`.

A genuinely fresh clone before a release:
`git clone . /tmp/objectify-verifyN && cd $_ && cargo build --release && cargo test --release && npm ci && npm run build && npm test --workspace @johnhenry/objectify-js`.

## Repo-specific gotchas

- **`src/main.rs` is intentionally one file.** Per `CONTRIBUTING.md`: "the
  codebase is intentionally kept in a single file. Resist splitting it
  unless a module becomes independently testable and reusable." Don't
  "clean up" by splitting modules as a drive-by change.
- **Deno is optional, and exactly one code path may touch it.** `get`,
  `set`, `list`, `log`, `diff`, `rewind`, `fork`, and `gc` must never
  require Deno. Only `use <id> <method>` (class method dispatch for
  TypeScript classes) does. A change that makes any other command depend on
  Deno breaks a documented invariant, not just a nice-to-have.
- **TypeScript schema extraction runs with broader Deno permissions than
  the class's own method calls** (`extract_ts_schema` always grants
  unconditional `--allow-env`/`--allow-net`, unscoped by `class.json`,
  because `ts-json-schema-generator` needs them to resolve `npm:` imports).
  See README's "Security model" before assuming a class's `class.json`
  fully bounds everything that runs on its behalf.
- **Python classes require `--allow-unsandboxed-python` by design, on
  purpose, everywhere they'd otherwise execute** (`create --class=<Py>` for
  schema extraction, `use <id> <method>` for calls). This is a deliberate
  fail-fast gate (fixed in 28a1fff, PR #10), not a bug to "fix" by making
  it more convenient — see README's "Class permissions" and "Security
  model".
- **Class names are validated against path traversal at intake**
  (`validate_class_name`) — don't bypass this by constructing a class file
  path from user input anywhere else in the codebase without the same
  check.
- **`npm/*` platform packages are not npm-workspace members and have no
  `npm install`/`build`/`test` of their own.** They're populated with a
  cross-compiled binary and a generated `checksums.json` only inside
  `release.yml`; don't add them to root `workspaces` or expect `npm run
  build` to touch them.
- **`CONTRIBUTING.md`'s "Making a release" section is stale.** It describes
  `git tag v0.x.y && git push --tags` and four platforms
  (`x86_64`/`aarch64-linux`, `x86_64`/`aarch64-darwin`); the actual
  mechanism is a **published GitHub Release** (`release.yml`'s trigger,
  also exposed as `workflow_dispatch` for retries) building **five**
  platforms including `win32-x64` — macOS is currently disabled in the
  build/publish workflow (see `release.yml`'s own header comment), not the
  four CONTRIBUTING.md lists. Follow `release.yml` and this file, not
  CONTRIBUTING.md's release steps, until that file is updated separately.
- **The Rust crate's version (`Cargo.toml`, currently `0.1.0`) and the npm
  packages' version (currently `0.0.0`, restarted on npm-scope migration)
  are independent version lines.** Don't assume bumping one bumps the
  other, and don't read the npm `0.0.0` as "less mature than the Rust
  0.1.0" — see `CHANGELOG.md`'s `0.0.0` entry.

## New-package definition of done

There is no registry of packages to add to in the usual monorepo sense —
the npm platform packages are generated by `release.yml`, not hand-created.
The one case that applies: adding a new supported platform means:
- A new build target added to `release.yml`'s build matrix and a new
  `npm/<platform>/package.json` manifest (copy an existing platform
  package's shape: name, `os`/`cpu` fields, `files`).
- The new package added to `npm/objectify/package.json`'s
  `optionalDependencies` (pinned exact, matching every other platform
  entry) and to its shim's platform-detection logic.
- `npm/objectify/README.md` updated to list the new platform as supported.

## Non-goals

No daemon, no cron, no background process, no delta/diff-based storage
(every write is a full snapshot — see README's "SQLite schema"), and no
merge semantics on `set` (always full replacement). These are stated design
constraints in `CONTRIBUTING.md`, not omissions to fill in.

## Releases

**Rust crate:** bump `version` in `Cargo.toml`, add a `CHANGELOG.md` entry.
The crate is not published to crates.io; its version is what
`objectify --version` reports and what's embedded in release binaries.

**npm packages:** bump `version` in `npm/objectify/package.json` and all
five `npm/<platform>/package.json` files to the same value, and update the
`optionalDependencies` versions in `npm/objectify/package.json` to match
(pinned exact, not a caret range — see `npm/objectify/README.md`). Commit
that as its own PR, merge, then `gh release create v<version>` — the
published-release event triggers `.github/workflows/release.yml`, which
cross-compiles the binary for each platform, populates each `npm/<platform>`
package with its binary + `checksums.json`, and publishes all six npm
packages with `--provenance`. Also exposed as `workflow_dispatch` for
re-running a single failed platform's publish step without redoing the
whole release. `packages/objectify-js` publishes separately via
`.github/workflows/objectify-js-publish.yml`, gated on the npm-idempotency
guard (skips if the version is already on npm).
