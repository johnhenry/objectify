# Changelog

All notable changes to objectify will be documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/). Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## 0.0.0 — npm package rename: the CLI shim is now `objectify-cli` (2026-09-26)

The CLI's npm install shim and its five platform packages are renamed from
`@johnhenry/objectify`/`@johnhenry/objectify-<platform>` to
`@johnhenry/objectify-cli`/`@johnhenry/objectify-cli-<platform>`. The plain
`@johnhenry/objectify` name is now the TypeScript adapter package
(`packages/objectify`, formerly published for a few minutes as
`@johnhenry/objectify-js` before this correction) — a cleaner split now that
both npm-side packages exist for real, rather than the CLI having first
claim on the unqualified name just because it's the project's primary
artifact. The actual CLI command you run is unaffected either way: it's
still `objectify`, only the npm *package* name changed. No functional
change to either package's code.

## 0.0.0 — npm scope migration, Python support, and a security audit (2026-08-25 – 2026-08-26)

The unscoped name `objectify` on npm belongs to an unrelated third party —
this was never published under any name before. First npm distribution, as
`@johnhenry/objectify` (the CLI, via a prebuilt-binary shim and five
platform packages) and `@johnhenry/objectify-js` (a TypeScript adapter).
Version restarts at `0.0.0`: a new address and era, not a maturity signal
relative to the `0.1.0` Rust crate below, which is unaffected by the npm
scope and continues to track its own version independently.

Alongside the scope migration, this window added Python class support (not
present in `0.1.0`) and closed four real findings from a security audit.

### Security fixes

- **Python class execution required no acknowledgment of its lack of
  sandboxing.** Python classes run as plain, unrestricted subprocesses
  (unlike TypeScript classes, which run under Deno's permission model) —
  running one silently would mean full filesystem/network/process access
  with no signal to the caller. `create`/`use` now refuse to execute a
  Python class at all without an explicit `--allow-unsandboxed-python` flag,
  failing fast with an error that states exactly what is being granted.
  Fixed in 28a1fff (fixes #6, #7); shipped via PR #10.
- **Class names were not validated against path traversal.** An unsanitized
  `--class` name could escape the classes directory. `validate_class_name`
  now rejects any class name containing a path separator or `..` segment,
  checked at intake before the name reaches any filesystem operation. Fixed
  in 28a1fff (fixes #6, #7); shipped via PR #10.
- **Published npm packages carried no provenance or integrity
  verification.** `release.yml` now publishes every package with
  `npm publish --provenance`, and the binary distribution ships a
  `checksums.json` manifest verified before a platform-specific binary is
  used — see `npm/objectify/README.md`. Fixed in af0bc8a (fixes #5); shipped
  via PR #10.
- **`objectify-js`'s SQLite store did not check schema compatibility on
  open.** Opening a store written by an incompatible version could silently
  misread it. `objectify-js` now validates the store's schema on open and
  fails clearly instead. Fixed in 9928334 (fixes #8); shipped via PR #10.

### Added

- Python class support: `DoBase[StateType]` classes using Pydantic v1/v2 or
  plain dataclasses, with the same CLI surface as TypeScript classes (see
  README, "Writing classes" → "Python").
- `@johnhenry/objectify-js`, a TypeScript adapter package
  (`packages/objectify-js`), adopted into the `@johnhenry` npm scope.
- npm binary distribution: `@johnhenry/objectify` (a small Node shim) plus
  five platform-specific optional-dependency packages
  (`@johnhenry/objectify-{darwin,linux,win32}-{arm64,x64}`), so
  `npm install -g @johnhenry/objectify` needs no Rust toolchain.
- Runnable, self-checking examples for both the CLI and the JS adapter.
- Class introspection (`objectify classes [<name>]`) and random hex object
  IDs with git-style prefix resolution.
- Starlight documentation site section at
  [opensource.johnhenry.me/objectify](https://opensource.johnhenry.me/objectify/).

### Housekeeping

- CI: macOS dropped from the per-PR Rust test matrix (still built and
  tested for real in `release.yml`, which produces the actual distributable
  binaries) to avoid the scarcest/slowest-to-schedule hosted runners gating
  every push. `publish.yml` (npm side) gained an idempotency guard that
  skips a package already at its published version.

## [0.1.0] — 2026-03-12

Initial release.

### Added

- `objectify init` — create `.objectify/` locally or globally with `--global`
- `objectify create` — create objects with optional description, class, and expiry duration
- `objectify destroy` — permanently delete an object and all its history
- `objectify inspect` — print object metadata as JSON
- `objectify list` — list objects with filtering by class, expiry, date range; human table or JSON output
- `objectify use <id> get` — read current state or any past version with `--at=<version>`
- `objectify use <id> set` — full-replacement state write with optional schema validation
- `objectify use <id> <method>` — call a user-defined TypeScript class method via Deno subprocess
- `objectify log` — show version history (human table when TTY, JSON when piped)
- `objectify diff` — RFC 6902 JSON Patch between any two versions
- `objectify rewind` — restore state to a past version (non-destructive, writes new version)
- `objectify fork` — clone an object into a new independent object, optionally at a past version
- `objectify gc` — delete all expired objects permanently
- ULID-based IDs with git-style prefix resolution (minimum 4-char prefix)
- Full-snapshot SQLite history — every write recorded, no delta compression
- TypeScript class system with `DoBase<T>`, injected `get`/`set`, arrow-function methods
- JSON Schema extraction via `ts-json-schema-generator` at class creation time
- Schema validation on every `set` (direct and via method)
- Deno permission sandboxing with `class.json` sidecar for opt-in extensions
- Path token expansion in `class.json`: `$HOME`, `$CWD`, `$OBJECTIFY_DIR`, `$TMPDIR`
- Lazy GC — expired objects still respond with a warning, only deleted on `gc`
- TTY-aware output — human-readable tables when interactive, JSON when piped
- Static binary — SQLite bundled, no system dependencies
