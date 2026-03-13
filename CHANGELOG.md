# Changelog

All notable changes to objectify will be documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/). Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

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
