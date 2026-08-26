# @johnhenry/objectify

`objectify` — persistent, versioned JSON objects backed by SQLite. A stateful,
agent-friendly CLI: `create` an object, `use` it to read/write state, `log` /
`diff` / `rewind` through its version history, `fork` an independent copy.

Full CLI reference, architecture notes, and "designed for agents" docs live at
**https://opensource.johnhenry.me/objectify/**.

> **This is the first npm distribution of `objectify`.** The CLI has existed
> as a `cargo build --release` / GitHub-release binary before now — this
> package (and its five per-platform siblings) is a new packaging story, not
> an import of a previously-published npm package. Versioning starts at
> `0.0.0`, matching this project's usual convention for a package's first
> appearance under the `@johnhenry` scope, extended here to cover "first npm
> appearance at all," since there's no prior npm version to restart from.

## Install

```sh
npm install -g @johnhenry/objectify
```

or run it without installing:

```sh
npx @johnhenry/objectify --help
```

## How this works

This package is a thin ~100-line Node.js shim (`bin/objectify.js`). It ships
no compiled code itself — it detects your OS/architecture at runtime and
delegates to the real binary, which lives in one of five tiny per-platform
packages:

| Package | Platform |
|---|---|
| `@johnhenry/objectify-darwin-arm64` | macOS, Apple Silicon |
| `@johnhenry/objectify-darwin-x64` | macOS, Intel |
| `@johnhenry/objectify-linux-x64` | Linux, x64 (glibc) |
| `@johnhenry/objectify-linux-arm64` | Linux, arm64 (glibc) |
| `@johnhenry/objectify-win32-x64` | Windows, x64 |

`npm install` only downloads the one matching your machine — it's listed as
an `optionalDependency` with `os`/`cpu` fields, which is how npm decides which
optional dependencies to actually fetch. This is the same pattern used by
`esbuild`, `@swc/core`, and `turbo`. No postinstall script runs and no network
request happens beyond npm's own package resolution — see
[`bin/objectify.js`](./bin/objectify.js) for the full resolution logic and
error messages if the right platform package didn't get installed.

## Platform support

Linux `x64`/`arm64` builds target glibc; there is currently no separate
`musl` build (e.g. for Alpine-based Docker images). If you need one, please
open an issue.

## License

MIT — see [LICENSE](./LICENSE).
