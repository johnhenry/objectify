# Shared helper for objectify examples. Source this; do not run it directly.
#
# Provides:
#   obj <args...>   Run the objectify CLI, preferring the release binary if built,
#                   otherwise falling back to `cargo run --release --quiet --`.
#   REPO_ROOT       Absolute path to the repository root.
#
# Usage in an example script:
#   source "$(dirname "$0")/_lib.sh"
#   obj init

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -x "$REPO_ROOT/target/release/objectify" ]; then
  obj() { "$REPO_ROOT/target/release/objectify" "$@"; }
else
  if ! command -v cargo >/dev/null 2>&1; then
    echo "error: neither target/release/objectify nor cargo found." >&2
    echo "Build the binary first: cargo build --release (in $REPO_ROOT)" >&2
    exit 1
  fi
  obj() { cargo run --manifest-path "$REPO_ROOT/Cargo.toml" --release --quiet -- "$@"; }
fi

# The CLI prints the new object ID as a JSON string (quoted) when stdout is not
# a TTY. This helper captures it as a plain shell string.
create_id() { obj create "$@" | tr -d '"'; }
