use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use rand::Rng;

// ─── CLI ─────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "objectify",
    about = "Persistent, versioned JSON objects backed by SQLite",
    long_about = "Persistent, versioned JSON objects backed by SQLite.\n\n\
        Every object is an append-only log of full JSON snapshots. \
        Objects can have an optional TypeScript or Python class that defines \
        typed state and callable methods.\n\n\
        IDs are random hex strings. Any unique prefix resolves to the full ID — same model as git.\n\n\
        Classes are .ts or .py files in .objectify/classes/. TypeScript runs under \
        Deno (sandboxed); Python runs under python3 (unsandboxed). \
        Both support async methods.",
    after_help = "EXAMPLES:\n  \
        objectify init                                   # init in current directory\n  \
        objectify create \"my config\"                    # untyped object\n  \
        objectify create \"tasks\" --class=TaskList        # typed object\n  \
        objectify create \"temp\" --expire=7d              # expires in 7 days\n  \
        objectify use 3fa8 set '{\"theme\":\"dark\"}'       # replace state\n  \
        objectify use 3fa8 get                           # read current state\n  \
        objectify use 3fa8 add -p:title \"write tests\"   # call class method\n  \
        objectify log 3fa8                               # version history\n  \
        objectify diff 3fa8 1 3                          # what changed\n  \
        objectify rewind 3fa8 2                          # restore to v2\n  \
        objectify fork 3fa8                              # new independent copy\n  \
        objectify gc                                     # delete expired objects",
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize objectify in the current directory (or globally)
    ///
    /// Creates a .objectify/ directory with a classes/ subfolder and an empty SQLite database.
    /// Walks up from the current directory to find an existing store; if none is found,
    /// creates one here. Use --global to create in ~/.objectify/ instead.
    #[command(after_help = "EXAMPLES:\n  \
        objectify init             # creates .objectify/ in current directory\n  \
        objectify init --global    # creates ~/.objectify/ (available everywhere)")]
    Init {
        /// Create the store in ~/.objectify/ instead of the current directory
        #[arg(long)]
        global: bool,
    },

    /// Create a new object and print its short ID
    ///
    /// Every object gets a random ID. The minimum unambiguous prefix (usually 4 chars)
    /// is printed — this is what you pass to all other commands.
    ///
    /// If --class is given, objectify looks for <ClassName>.ts or <ClassName>.py in
    /// .objectify/classes/ and stores the class name on the object. Schema is extracted
    /// once at creation and used to validate all future writes.
    #[command(after_help = "EXPIRY FORMAT:\n  \
        Suffix  Unit\n  \
        s       seconds   (e.g. 30s)\n  \
        m       minutes   (e.g. 15m)\n  \
        h       hours     (e.g. 2h)\n  \
        d       days      (e.g. 7d)\n  \
        w       weeks     (e.g. 2w)\n\n\
        EXAMPLES:\n  \
        objectify create                               # anonymous untyped object\n  \
        objectify create \"sprint tasks\"               # with description\n  \
        objectify create \"sprint tasks\" --class=TaskList\n  \
        objectify create \"temp flags\" --expire=7d")]
    Create {
        /// Human-readable label for this object (stored but not used as an ID)
        #[arg(value_name = "DESCRIPTION")]
        description: Option<String>,

        /// Associate a class with this object. File must exist as <CLASS>.ts or <CLASS>.py
        /// in the classes directory. Schema is extracted and stored at creation time.
        #[arg(long, value_name = "CLASS")]
        class: Option<String>,

        /// Automatically expire the object after this duration (e.g. 30s, 15m, 2h, 7d, 2w).
        /// Expired objects emit a warning but still respond until `objectify gc` is run.
        #[arg(long, value_name = "DURATION")]
        expire: Option<String>,
    },

    /// Permanently delete an object and all its version history
    ///
    /// This is irreversible. The object's entire event log is deleted via CASCADE.
    /// Use `objectify gc` to bulk-delete expired objects instead.
    #[command(after_help = "EXAMPLES:\n  objectify destroy 3fa8")]
    Destroy {
        /// ID prefix or full ID of the object to delete
        #[arg(value_name = "ID")]
        id: String,
    },

    /// Show metadata for an object
    ///
    /// Returns a JSON object with the full ID, short ID, class name, description,
    /// version count, creation timestamp, and expiry.
    #[command(after_help = "EXAMPLES:\n  objectify inspect 3fa8")]
    Inspect {
        /// ID prefix or full ID
        #[arg(value_name = "ID")]
        id: String,
    },

    /// List objects in the store
    ///
    /// When stdout is a TTY, outputs a human-readable table. When piped or redirected
    /// (or with --json), outputs a JSON array. Defaults to the 50 most recently created
    /// objects, excluding expired ones.
    #[command(after_help = "SINCE FORMAT:\n  \
        Accepts a relative duration (e.g. 2d, 1w) or an ISO 8601 date/datetime.\n\n\
        EXAMPLES:\n  \
        objectify list\n  \
        objectify list --class=TaskList\n  \
        objectify list --since=7d\n  \
        objectify list --expired --limit=100\n  \
        objectify list --json | jq '.[].id'")]
    List {
        /// Only show objects with this class name
        #[arg(long, value_name = "CLASS")]
        class: Option<String>,

        /// Include expired objects (normally hidden)
        #[arg(long)]
        expired: bool,

        /// Only show objects created on or after this date/duration (e.g. 2d, 2025-01-01)
        #[arg(long, value_name = "DATE_OR_DURATION")]
        since: Option<String>,

        /// Maximum number of results to return
        #[arg(long, default_value = "50", value_name = "N")]
        limit: u64,

        /// Skip this many results (for pagination)
        #[arg(long, default_value = "0", value_name = "N")]
        offset: u64,

        /// Force JSON output even when stdout is a TTY
        #[arg(long)]
        json: bool,
    },

    /// Get/set state or call a class method on an object
    ///
    /// Three built-in subcommands:
    ///
    ///   get [--at=VERSION]          Read current state (or a specific version)
    ///   set <JSON>                  Replace state entirely (validates against schema)
    ///   <method> [INPUT]            Call a class method (requires Deno or Python)
    ///
    /// METHOD INPUT FORMS (all equivalent):
    ///
    ///   Positional JSON:            use <id> add '{"title":"x"}'
    ///   Full JSON flag:             use <id> add -p '{"title":"x"}'
    ///                               use <id> add --parameter '{"title":"x"}'
    ///   Key-value flag:             use <id> add -p:title "x"
    ///                               use <id> add --parameter:title "x"
    ///   Multiple kv pairs:          use <id> add -p:title "x" -p:priority 1
    ///
    /// Values in -p:key VALUE are JSON-parsed first (numbers, booleans, arrays,
    /// objects all work). Falls back to a plain string if parsing fails.
    #[command(
        override_usage = "objectify use <ID> get [--at=VERSION]\n       \
                          objectify use <ID> set <JSON>\n       \
                          objectify use <ID> <METHOD> [INPUT | -p:key value ...]",
        after_help = "EXAMPLES:\n  \
        objectify use 3fa8 get                           # current state\n  \
        objectify use 3fa8 get --at=2                    # state at version 2\n  \
        objectify use 3fa8 set '{\"theme\":\"dark\"}'        # replace state\n  \
        objectify use 3fa8 add '{\"title\":\"write tests\"}' # call method with JSON\n  \
        objectify use 3fa8 add -p:title \"write tests\"   # same, kv style\n  \
        objectify use 3fa8 add -p:title \"x\" -p:priority 1  # multiple kv\n  \
        objectify use 3fa8 pending                       # method with no input\n  \
        objectify use 3fa8 help                          # list available methods",
    )]
    Use {
        /// ID prefix or full ID of the object
        #[arg(value_name = "ID")]
        id: String,

        /// Subcommand and arguments: get [--at=N] | set <JSON> | <method> [input]
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, value_name = "ARGS")]
        args: Vec<String>,
    },

    /// Show the version history of an object
    ///
    /// Each row shows the version number, the method that wrote it (create / set / method name),
    /// and how long ago it was written. When piped, outputs a JSON array.
    #[command(after_help = "EXAMPLES:\n  objectify log 3fa8\n  objectify log 3fa8 | jq '.[-1]'")]
    Log {
        /// ID prefix or full ID
        #[arg(value_name = "ID")]
        id: String,
    },

    /// Show an RFC 6902 JSON Patch diff between two versions
    ///
    /// Outputs an array of patch operations (add / remove / replace / move / copy / test).
    /// Useful for auditing what changed between two steps.
    #[command(after_help = "EXAMPLES:\n  \
        objectify diff 3fa8 1 3    # what changed from v1 to v3\n  \
        objectify diff 3fa8 2 2    # empty diff (same version)")]
    Diff {
        /// ID prefix or full ID
        #[arg(value_name = "ID")]
        id: String,

        /// Version number to diff from
        #[arg(value_name = "V1")]
        v1: u64,

        /// Version number to diff to
        #[arg(value_name = "V2")]
        v2: u64,
    },

    /// Restore an object to a previous version (non-destructive)
    ///
    /// Reads the state at VERSION and writes it as a new event. History is never deleted —
    /// the rewind itself becomes the next version. To see available versions, run `log` first.
    #[command(after_help = "EXAMPLES:\n  \
        objectify rewind 3fa8 2    # restore to v2; new version is written\n  \
        objectify log 3fa8         # confirm the rewind appears in history")]
    Rewind {
        /// ID prefix or full ID
        #[arg(value_name = "ID")]
        id: String,

        /// Version number to restore to
        #[arg(value_name = "VERSION")]
        version: u64,
    },

    /// Fork an object into a new independent object
    ///
    /// Copies state (and class / description / schema) into a fresh object with a new ID.
    /// After forking, mutations to either object do not affect the other.
    /// Use --at to fork from a specific version rather than the latest.
    #[command(after_help = "EXAMPLES:\n  \
        objectify fork 3fa8           # fork from latest state\n  \
        objectify fork 3fa8 --at=2   # fork from version 2")]
    Fork {
        /// ID prefix or full ID of the source object
        #[arg(value_name = "ID")]
        id: String,

        /// Fork from this version instead of the latest
        #[arg(long, value_name = "VERSION")]
        at: Option<u64>,
    },

    /// Permanently delete all expired objects and their history
    ///
    /// Expired objects normally continue to respond (with a stderr warning). Run gc to
    /// actually remove them. Prints the number of objects deleted.
    #[command(after_help = "EXAMPLES:\n  objectify gc")]
    Gc,

    /// List available classes in the classes directory
    ///
    /// Scans the classes directory for .ts and .py files and prints one entry per class.
    /// Each entry includes the class name, language, and the number of objects currently
    /// using that class.
    ///
    /// When stdout is a TTY, outputs a human-readable table. Otherwise, outputs a JSON array.
    /// If both a .ts and .py file exist for the same name, both are shown separately — but
    /// TypeScript takes precedence at runtime.
    #[command(
        name = "classes",
        after_help = "EXAMPLES:\n  \
            objectify classes              # list all classes\n  \
            objectify classes --json       # machine-readable output\n  \
            objectify classes TaskList     # describe methods on TaskList\n  \
            objectify list --class=TaskList  # find objects using a specific class",
    )]
    Classes {
        /// Show methods for a specific class (by name)
        #[arg(value_name = "CLASS_NAME")]
        name: Option<String>,

        /// Force JSON output even when stdout is a TTY
        #[arg(long)]
        json: bool,
    },

    /// Install the objectify skill into a skills directory
    ///
    /// Writes SKILL.md (embedded in this binary) to a `objectify/` subdirectory
    /// of the target skills directory. By default, installs to `.skills/skills/`
    /// walking up from the current directory, then falls back to `~/.skills/skills/`.
    /// Use --path to install to a specific directory instead.
    ///
    /// This is the equivalent of `objectify init` but for the skill file — run it
    /// once after installing the binary to make the skill available to AI agents.
    #[command(
        name = "skill",
        after_help = "EXAMPLES:\n  \
            objectify skill install                    # auto-detect skills dir\n  \
            objectify skill install --path ~/.skills/skills  # explicit path\n  \
            objectify skill install --global           # install to ~/.skills/skills/",
    )]
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
}

#[derive(Subcommand)]
enum SkillAction {
    /// Write SKILL.md to the skills directory so AI agents can discover objectify
    #[command(
        after_help = "EXAMPLES:\n  \
            objectify skill install\n  \
            objectify skill install --global\n  \
            objectify skill install --path /path/to/skills",
    )]
    Install {
        /// Install to ~/.skills/skills/ regardless of local skills directories
        #[arg(long)]
        global: bool,

        /// Install to this specific directory (creates objectify/ subdirectory inside it)
        #[arg(long, value_name = "DIR")]
        path: Option<PathBuf>,
    },
}

// ─── OUTPUT ──────────────────────────────────────────────────────────────────

fn print_json<T: Serialize>(val: &T) {
    println!("{}", serde_json::to_string_pretty(val).unwrap());
}

fn is_tty() -> bool {
    std::io::stdout().is_terminal()
}

// ─── CONTEXT ─────────────────────────────────────────────────────────────────

struct ObjectifyContext {
    dir: PathBuf,
    classes_dir: PathBuf,
    db_path: PathBuf,
}

impl ObjectifyContext {
    fn resolve() -> Result<Self> {
        let dir = Self::find_dir()?;
        let classes_dir = dir.join("classes");
        let db_path = dir.join("objectify.db");
        Ok(Self { dir, classes_dir, db_path })
    }

    fn find_dir() -> Result<PathBuf> {
        let mut d = std::env::current_dir()?;
        loop {
            let candidate = d.join(".objectify");
            if candidate.is_dir() {
                return Ok(candidate);
            }
            if !d.pop() {
                break;
            }
        }
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow!("cannot determine home directory"))?;
        let global = home.join(".objectify");
        if global.is_dir() {
            return Ok(global);
        }
        bail!("no .objectify/ found. Run: objectify init")
    }

    fn open_db(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        init_schema(&conn)?;
        Ok(conn)
    }
}

// ─── SCHEMA ──────────────────────────────────────────────────────────────────

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS objects (
            id          TEXT PRIMARY KEY,
            class       TEXT,
            description TEXT,
            schema      TEXT,
            created_at  TEXT NOT NULL,
            expires_at  TEXT
        );
        CREATE TABLE IF NOT EXISTS events (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            object_id   TEXT NOT NULL REFERENCES objects(id) ON DELETE CASCADE,
            version     INTEGER NOT NULL,
            method      TEXT NOT NULL,
            state       TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            UNIQUE(object_id, version)
        );
        CREATE INDEX IF NOT EXISTS idx_events_object_version ON events(object_id, version);
        CREATE INDEX IF NOT EXISTS idx_objects_class ON objects(class);
        CREATE INDEX IF NOT EXISTS idx_objects_expires ON objects(expires_at)
            WHERE expires_at IS NOT NULL;",
    )?;
    Ok(())
}

// ─── IDs ─────────────────────────────────────────────────────────────────────

fn new_id() -> String {
    let bytes: [u8; 16] = rand::rng().random();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn resolve_id(conn: &Connection, prefix: &str) -> Result<String> {
    let prefix_lower = prefix.to_lowercase();
    let mut stmt = conn.prepare("SELECT id FROM objects WHERE id LIKE ?1 || '%'")?;
    let ids: Vec<String> = stmt
        .query_map(params![prefix_lower], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    match ids.len() {
        0 => bail!("object not found: {}", prefix),
        1 => Ok(ids.into_iter().next().unwrap()),
        _ => {
            let shorts: Vec<String> = ids.iter().map(|id| display_id(conn, id)).collect();
            bail!("ambiguous id prefix '{}': matches {}", prefix, shorts.join(", "))
        }
    }
}

/// Compute the minimum unique prefix for display (4-char minimum).
fn display_id(conn: &Connection, full_id: &str) -> String {
    let lower = full_id.to_lowercase();
    for len in 4..=lower.len() {
        let prefix = &lower[..len];
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM objects WHERE id LIKE ?1 || '%'",
                params![prefix],
                |row| row.get(0),
            )
            .unwrap_or(2);
        if count == 1 {
            return prefix.to_string();
        }
    }
    lower
}

// ─── TIME UTILITIES ──────────────────────────────────────────────────────────

fn parse_expiry_duration(s: &str) -> Result<String> {
    if s.is_empty() {
        bail!("empty expiry string");
    }
    let unit = s.chars().last().unwrap();
    let amount_str = &s[..s.len() - 1];
    let amount: i64 = amount_str
        .parse()
        .map_err(|_| anyhow!("invalid expiry: {}", s))?;
    let duration = match unit {
        's' => Duration::seconds(amount),
        'm' => Duration::minutes(amount),
        'h' => Duration::hours(amount),
        'd' => Duration::days(amount),
        'w' => Duration::weeks(amount),
        _ => bail!("invalid expiry unit '{}'. Use: s, m, h, d, w", unit),
    };
    Ok((Utc::now() + duration).to_rfc3339())
}

fn parse_since_filter(s: &str) -> Result<String> {
    if let Some(unit) = s.chars().last() {
        if "smhdw".contains(unit) {
            let amount_str = &s[..s.len() - 1];
            if let Ok(amount) = amount_str.parse::<i64>() {
                let duration = match unit {
                    's' => Duration::seconds(amount),
                    'm' => Duration::minutes(amount),
                    'h' => Duration::hours(amount),
                    'd' => Duration::days(amount),
                    'w' => Duration::weeks(amount),
                    _ => unreachable!(),
                };
                return Ok((Utc::now() - duration).to_rfc3339());
            }
        }
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc).to_rfc3339());
    }
    if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = nd.and_hms_opt(0, 0, 0).unwrap().and_utc();
        return Ok(dt.to_rfc3339());
    }
    bail!("invalid date/duration: {}", s)
}

fn human_relative(dt_str: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(dt_str) else {
        return dt_str.to_string();
    };
    let secs = Utc::now()
        .signed_duration_since(dt.with_timezone(&Utc))
        .num_seconds();
    if secs < 0 {
        return "just now".to_string();
    }
    if secs < 60 { format!("{} secs ago", secs) }
    else if secs < 3600 { format!("{} mins ago", secs / 60) }
    else if secs < 86400 { format!("{} hours ago", secs / 3600) }
    else if secs < 86400 * 7 { format!("{} days ago", secs / 86400) }
    else if secs < 86400 * 30 { format!("{} weeks ago", secs / (86400 * 7)) }
    else { dt.format("%Y-%m-%d").to_string() }
}

fn human_expiry(expires_at: Option<&str>) -> String {
    let Some(s) = expires_at else {
        return "never".to_string();
    };
    let Ok(dt) = DateTime::parse_from_rfc3339(s) else {
        return s.to_string();
    };
    let secs = dt.with_timezone(&Utc).signed_duration_since(Utc::now()).num_seconds();
    if secs < 0 { "expired".to_string() }
    else if secs < 60 { format!("in {} secs", secs) }
    else if secs < 3600 { format!("in {} mins", secs / 60) }
    else if secs < 86400 { format!("in {} hours", secs / 3600) }
    else if secs < 86400 * 7 { format!("in {} days", secs / 86400) }
    else { format!("in {} weeks", secs / (86400 * 7)) }
}

// ─── DB HELPERS ──────────────────────────────────────────────────────────────

#[derive(Debug)]
struct ObjectRow {
    id: String,
    class: Option<String>,
    description: Option<String>,
    schema: Option<String>,
    created_at: String,
    expires_at: Option<String>,
}

fn get_object(conn: &Connection, full_id: &str) -> Result<ObjectRow> {
    conn.query_row(
        "SELECT id, class, description, schema, created_at, expires_at FROM objects WHERE id = ?1",
        params![full_id],
        |row| Ok(ObjectRow {
            id: row.get(0)?,
            class: row.get(1)?,
            description: row.get(2)?,
            schema: row.get(3)?,
            created_at: row.get(4)?,
            expires_at: row.get(5)?,
        }),
    ).map_err(|e| anyhow!("object not found: {}", e))
}

fn version_count(conn: &Connection, object_id: &str) -> Result<u64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM events WHERE object_id = ?1",
        params![object_id],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}

fn get_state_at(conn: &Connection, object_id: &str, version: Option<u64>) -> Result<Value> {
    let state_str: String = match version {
        Some(v) => conn.query_row(
            "SELECT state FROM events WHERE object_id = ?1 AND version = ?2",
            params![object_id, v as i64],
            |row| row.get(0),
        ).map_err(|_| anyhow!("version {} not found", v))?,
        None => conn.query_row(
            "SELECT state FROM events WHERE object_id = ?1 ORDER BY version DESC LIMIT 1",
            params![object_id],
            |row| row.get(0),
        ).map_err(|_| anyhow!("no state found for object"))?,
    };
    Ok(serde_json::from_str(&state_str)?)
}

fn next_version(conn: &Connection, object_id: &str) -> Result<u64> {
    let max: Option<i64> = conn.query_row(
        "SELECT MAX(version) FROM events WHERE object_id = ?1",
        params![object_id],
        |row| row.get(0),
    )?;
    Ok(max.unwrap_or(0) as u64 + 1)
}

fn write_event(conn: &Connection, object_id: &str, method: &str, state: &Value) -> Result<u64> {
    let version = next_version(conn, object_id)?;
    let state_str = serde_json::to_string(state)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO events (object_id, version, method, state, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![object_id, version as i64, method, state_str, now],
    )?;
    Ok(version)
}

fn is_expired(obj: &ObjectRow) -> bool {
    if let Some(ref exp) = obj.expires_at {
        if let Ok(dt) = DateTime::parse_from_rfc3339(exp) {
            return Utc::now() > dt.with_timezone(&Utc);
        }
    }
    false
}

// ─── SCHEMA VALIDATION ───────────────────────────────────────────────────────

fn validate_state(schema_str: &str, state: &Value) -> Result<()> {
    let schema: Value = serde_json::from_str(schema_str)
        .map_err(|e| anyhow!("invalid stored schema: {}", e))?;
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|e| anyhow!("schema compile error: {}", e))?;
    if let Err(errors) = compiled.validate(state) {
        let msgs: Vec<String> = errors.map(|e| e.to_string()).collect();
        bail!("schema validation failed: {}", msgs.join("; "));
    }
    Ok(())
}

// ─── CLASS RUNNER ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum ClassLang {
    TypeScript,
    Python,
}

/// Locate a class file, checking .ts then .py. Returns the path and language.
fn find_class_file(ctx: &ObjectifyContext, class_name: &str) -> Result<(PathBuf, ClassLang)> {
    let ts = ctx.classes_dir.join(format!("{}.ts", class_name));
    if ts.exists() {
        return Ok((ts, ClassLang::TypeScript));
    }
    let py = ctx.classes_dir.join(format!("{}.py", class_name));
    if py.exists() {
        return Ok((py, ClassLang::Python));
    }
    bail!(
        "class '{}' not found. Expected {}.ts or {}.py in {}",
        class_name,
        class_name,
        class_name,
        ctx.classes_dir.display()
    )
}

fn find_deno() -> Result<PathBuf> {
    which::which("deno")
        .map_err(|_| anyhow!("class execution requires Deno. Install from https://deno.land"))
}

fn find_python() -> Result<PathBuf> {
    which::which("python3")
        .or_else(|_| which::which("python"))
        .map_err(|_| anyhow!("class execution requires Python 3. Install from https://python.org"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClassResult {
    result: Value,
    state_changed: bool,
    state: Option<Value>,
}

fn generate_runner_script(class_file: &Path) -> String {
    let class_path = class_file.display();
    format!(
        r#"import UserClass from '{class_path}';

const input = JSON.parse(Deno.env.get('OBJECTIFY_INPUT') || 'null');
const currentState = JSON.parse(Deno.env.get('OBJECTIFY_STATE') || '{{}}');

let pendingState = currentState;
let stateChanged = false;

const instance = new UserClass();

(instance as any).get = async () => structuredClone(currentState);
(instance as any).set = async (next: unknown) => {{
  pendingState = next;
  stateChanged = true;
}};

const method = Deno.env.get('OBJECTIFY_METHOD')!;
const result = await (instance as any)[method](input);

const output = {{
  result: result ?? null,
  stateChanged,
  state: stateChanged ? pendingState : null,
}};

console.log(JSON.stringify(output));
"#
    )
}

fn generate_python_runner(class_file: &Path, class_name: &str) -> String {
    let class_path = class_file.display();
    // Use {{}} to escape braces in the format string for the Python dict literals
    format!(
        r#"import sys, asyncio, json, os, importlib.util, inspect, types, copy

try:
    import dataclasses as _dc
    _has_dc = True
except ImportError:
    _has_dc = False

# ── Inject fake 'objectify' module so user code can: from objectify import DoBase ──
class _DoBase:
    """Injected at runtime. Do not subclass directly — use DoBase[StateType]."""
    def __init__(self):
        self._get_fn = None
        self._set_fn = None

    async def get(self):
        return await self._get_fn()

    async def set(self, state):
        return await self._set_fn(state)

    # Allow DoBase[T] subscript syntax without runtime error
    def __class_getitem__(cls, _item):
        return cls

_objectify_mod = types.ModuleType('objectify')
_objectify_mod.DoBase = _DoBase
sys.modules['objectify'] = _objectify_mod

# ── Load the user's class file ──
_spec = importlib.util.spec_from_file_location("_user_class", "{class_path}")
_module = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_module)
UserClass = getattr(_module, "{class_name}")

# ── Environment ──
_input_raw  = json.loads(os.environ.get('OBJECTIFY_INPUT',  'null'))
_state_raw  = json.loads(os.environ.get('OBJECTIFY_STATE',  'null'))
_method_name = os.environ['OBJECTIFY_METHOD']

_pending_state = _state_raw
_state_changed = False

# ── Injected get / set ──
async def _get_impl():
    return _state_raw

def _serialize_state(s):
    """Convert typed state objects to plain JSON-compatible dicts."""
    if s is None:
        return None
    if hasattr(s, 'model_dump'):           # pydantic v2
        return s.model_dump(mode='json')
    if hasattr(s, 'dict'):                 # pydantic v1
        return s.dict()
    if _has_dc and _dc.is_dataclass(s) and not isinstance(s, type):
        return _dc.asdict(s)
    return s

async def _set_impl(next_state):
    global _pending_state, _state_changed
    _pending_state = _serialize_state(next_state)
    _state_changed = True

# ── Instantiate and inject ──
_instance = UserClass()
_instance._get_fn = _get_impl
_instance._set_fn = _set_impl

_method = getattr(_instance, _method_name)

# ── Call: unpack dict inputs as kwargs, pass scalars/lists positionally ──
def _invoke():
    if isinstance(_input_raw, dict):
        return _method(**_input_raw)
    elif _input_raw is None:
        return _method()
    else:
        return _method(_input_raw)

async def _main():
    if inspect.iscoroutinefunction(_method):
        return await _invoke()
    else:
        return _invoke()

_result = asyncio.run(_main())

def _serialize_result(obj):
    if obj is None:
        return None
    if hasattr(obj, 'model_dump'):
        return obj.model_dump(mode='json')
    if hasattr(obj, 'dict'):
        return obj.dict()
    if _has_dc and _dc.is_dataclass(obj) and not isinstance(obj, type):
        return _dc.asdict(obj)
    if isinstance(obj, (dict, list, str, int, float, bool)):
        return obj
    return str(obj)

print(json.dumps({{
    'result':       _serialize_result(_result),
    'stateChanged': _state_changed,
    'state':        _pending_state if _state_changed else None,
}}))
"#,
        class_path = class_path,
        class_name = class_name,
    )
}

fn expand_path_tokens(path: &str, ctx: &ObjectifyContext) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let tmpdir = std::env::temp_dir();
    path.replace("$HOME", &home.display().to_string())
        .replace("$CWD", &cwd.display().to_string())
        .replace("$OBJECTIFY_DIR", &ctx.dir.display().to_string())
        .replace("$TMPDIR", &tmpdir.display().to_string())
}

fn build_deno_flags(ctx: &ObjectifyContext, perms: &Value) -> Vec<String> {
    let deno_cache = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("deno");

    let mut flags = vec![
        format!(
            "--allow-read={},{}",
            ctx.classes_dir.display(),
            deno_cache.display()
        ),
        "--allow-env=OBJECTIFY_INPUT,OBJECTIFY_STATE,OBJECTIFY_METHOD".to_string(),
    ];

    if perms.is_null() || !perms.is_object() {
        return flags;
    }

    let expand = |val: &Value| -> Vec<String> {
        match val {
            Value::Bool(true) => vec![],  // wildcard — handled per flag
            Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|p| expand_path_tokens(p, ctx))
                .collect(),
            _ => vec![],
        }
    };

    if let Some(net) = perms.get("net") {
        match net {
            Value::Bool(true) => flags.push("--allow-net".to_string()),
            Value::Array(hosts) => {
                let h: Vec<&str> = hosts.iter().filter_map(|v| v.as_str()).collect();
                if !h.is_empty() {
                    flags.push(format!("--allow-net={}", h.join(",")));
                }
            }
            _ => {}
        }
    }

    if let Some(read) = perms.get("read") {
        flags.retain(|f| !f.starts_with("--allow-read"));
        match read {
            Value::Bool(true) => flags.push("--allow-read".to_string()),
            Value::Array(_) => {
                let paths = expand(read);
                flags.push(format!(
                    "--allow-read={},{},{}",
                    ctx.classes_dir.display(),
                    deno_cache.display(),
                    paths.join(",")
                ));
            }
            _ => {}
        }
    }

    if let Some(write) = perms.get("write") {
        match write {
            Value::Bool(true) => flags.push("--allow-write".to_string()),
            Value::Array(_) => {
                let paths = expand(write);
                if !paths.is_empty() {
                    flags.push(format!("--allow-write={}", paths.join(",")));
                }
            }
            _ => {}
        }
    }

    if let Some(env) = perms.get("env") {
        flags.retain(|f| f.starts_with("--allow-env"));
        let base = "OBJECTIFY_INPUT,OBJECTIFY_STATE,OBJECTIFY_METHOD";
        match env {
            Value::Bool(true) => flags.push("--allow-env".to_string()),
            Value::Array(vars) => {
                let extra: Vec<&str> = vars.iter().filter_map(|v| v.as_str()).collect();
                if extra.is_empty() {
                    flags.push(format!("--allow-env={}", base));
                } else {
                    flags.push(format!("--allow-env={},{}", base, extra.join(",")));
                }
            }
            _ => {}
        }
    }

    if let Some(run) = perms.get("run") {
        match run {
            Value::Bool(true) => flags.push("--allow-run".to_string()),
            Value::Array(bins) => {
                let b: Vec<&str> = bins.iter().filter_map(|v| v.as_str()).collect();
                if !b.is_empty() {
                    flags.push(format!("--allow-run={}", b.join(",")));
                }
            }
            _ => {}
        }
    }

    if let Some(sys) = perms.get("sys") {
        if let Value::Array(syscalls) = sys {
            let s: Vec<&str> = syscalls.iter().filter_map(|v| v.as_str()).collect();
            if !s.is_empty() {
                flags.push(format!("--allow-sys={}", s.join(",")));
            }
        }
    }

    flags
}

fn run_class_method(
    ctx: &ObjectifyContext,
    class_name: &str,
    method: &str,
    current_state: &Value,
    input: Option<&Value>,
) -> Result<ClassResult> {
    let (class_file, lang) = find_class_file(ctx, class_name)?;

    let state_json = serde_json::to_string(current_state)?;
    let input_json = input
        .map(|v| serde_json::to_string(v).unwrap())
        .unwrap_or_else(|| "null".to_string());

    match lang {
        ClassLang::TypeScript => {
            run_ts_method(ctx, class_name, &class_file, method, &state_json, &input_json)
        }
        ClassLang::Python => {
            run_py_method(class_name, &class_file, method, &state_json, &input_json)
        }
    }
}

fn run_ts_method(
    ctx: &ObjectifyContext,
    class_name: &str,
    class_file: &Path,
    method: &str,
    state_json: &str,
    input_json: &str,
) -> Result<ClassResult> {
    let deno = find_deno()?;

    let sidecar = ctx.classes_dir.join(format!("{}.json", class_name));
    let perms = if sidecar.exists() {
        let raw = std::fs::read_to_string(&sidecar)?;
        serde_json::from_str::<Value>(&raw)
            .map_err(|e| anyhow!("invalid {}.json: {}", class_name, e))?
    } else {
        Value::Null
    };

    let runner_script = generate_runner_script(class_file);
    use std::io::Write as IoWrite;
    let mut tmpfile = tempfile::Builder::new()
        .prefix("objectify-runner-")
        .suffix(".ts")
        .tempfile()?;
    write!(tmpfile, "{}", runner_script)?;

    let flags = build_deno_flags(ctx, &perms);
    let mut cmd = std::process::Command::new(&deno);
    cmd.arg("run");
    for flag in &flags { cmd.arg(flag); }
    cmd.arg(tmpfile.path());
    cmd.env("OBJECTIFY_STATE",  state_json);
    cmd.env("OBJECTIFY_METHOD", method);
    cmd.env("OBJECTIFY_INPUT",  input_json);

    invoke_runner(cmd, method)
}

fn run_py_method(
    class_name: &str,
    class_file: &Path,
    method: &str,
    state_json: &str,
    input_json: &str,
) -> Result<ClassResult> {
    let python = find_python()?;

    let runner_script = generate_python_runner(class_file, class_name);
    use std::io::Write as IoWrite;
    let mut tmpfile = tempfile::Builder::new()
        .prefix("objectify-runner-")
        .suffix(".py")
        .tempfile()?;
    write!(tmpfile, "{}", runner_script)?;

    let mut cmd = std::process::Command::new(&python);
    cmd.arg(tmpfile.path());
    cmd.env("OBJECTIFY_STATE",  state_json);
    cmd.env("OBJECTIFY_METHOD", method);
    cmd.env("OBJECTIFY_INPUT",  input_json);

    invoke_runner(cmd, method)
}

/// Shared: run a pre-built Command, capture output, parse ClassResult.
fn invoke_runner(mut cmd: std::process::Command, method: &str) -> Result<ClassResult> {
    let output = cmd.output().map_err(|e| anyhow!("failed to spawn runner: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("class method '{}' failed:\n{}", method, stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<ClassResult>(&stdout)
        .map_err(|e| anyhow!("invalid output from class method: {}. stdout: {}", e, stdout))
}

// ─── SCHEMA EXTRACTION ───────────────────────────────────────────────────────

/// Best-effort: extract a JSON Schema for the class's state type.
/// Returns None silently if the toolchain is unavailable or extraction fails.
fn extract_schema(ctx: &ObjectifyContext, class_name: &str) -> Result<Option<String>> {
    match find_class_file(ctx, class_name) {
        Ok((file, ClassLang::TypeScript)) => extract_ts_schema(&file),
        Ok((file, ClassLang::Python))     => extract_py_schema(&file, class_name),
        Err(_) => Ok(None),
    }
}

fn extract_ts_schema(class_file: &Path) -> Result<Option<String>> {
    let deno = match find_deno() {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };

    let extractor = format!(
        r#"import {{ createGenerator }} from "npm:ts-json-schema-generator";
const generator = createGenerator({{
  path: "{}",
  expose: "all",
  topRef: false,
  jsDoc: "extended",
}});
try {{
  const schema = generator.createSchema("*");
  console.log(JSON.stringify(schema));
}} catch (_e) {{
  console.log("null");
}}
"#,
        class_file.display()
    );

    use std::io::Write as IoWrite;
    let mut tmpfile = tempfile::Builder::new()
        .prefix("objectify-extractor-")
        .suffix(".ts")
        .tempfile()?;
    write!(tmpfile, "{}", extractor)?;

    let deno_cache = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("deno");

    let out = std::process::Command::new(&deno)
        .arg("run")
        .arg(format!(
            "--allow-read={},{},{}",
            class_file.parent().unwrap().display(),
            tmpfile.path().parent().unwrap().display(),
            deno_cache.display()
        ))
        .arg("--allow-env")
        .arg("--allow-net")
        .arg(tmpfile.path())
        .output();

    schema_from_output(out)
}

fn extract_py_schema(class_file: &Path, class_name: &str) -> Result<Option<String>> {
    let python = match find_python() {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    // Walk the class's __orig_bases__ to find DoBase[StateType], then call
    // model_json_schema() if it's a Pydantic model, or generate one from
    // dataclass fields via pydantic's TypeAdapter.
    let extractor = format!(
        r#"
import sys, json, types, importlib.util

class _DoBase:
    def __class_getitem__(cls, _item): return cls

_mod = types.ModuleType('objectify')
_mod.DoBase = _DoBase
sys.modules['objectify'] = _mod

_spec = importlib.util.spec_from_file_location("_uc", "{class_path}")
_module = importlib.util.module_from_spec(_spec)
try:
    _spec.loader.exec_module(_module)
except Exception as e:
    print("null"); sys.exit(0)

UserClass = getattr(_module, "{class_name}", None)
if UserClass is None:
    print("null"); sys.exit(0)

# Find the state type T from DoBase[T]
_state_type = None
for base in getattr(UserClass, '__orig_bases__', []):
    args = getattr(base, '__args__', ())
    if args:
        _state_type = args[0]
        break

if _state_type is None:
    print("null"); sys.exit(0)

# pydantic v2
if hasattr(_state_type, 'model_json_schema'):
    print(json.dumps(_state_type.model_json_schema()))
    sys.exit(0)

# pydantic v1
if hasattr(_state_type, 'schema'):
    print(json.dumps(_state_type.schema()))
    sys.exit(0)

# dataclass via pydantic TypeAdapter (pydantic v2)
try:
    import dataclasses
    from pydantic import TypeAdapter
    if dataclasses.is_dataclass(_state_type):
        print(json.dumps(TypeAdapter(_state_type).json_schema()))
        sys.exit(0)
except Exception:
    pass

print("null")
"#,
        class_path = class_file.display(),
        class_name = class_name,
    );

    use std::io::Write as IoWrite;
    let mut tmpfile = tempfile::Builder::new()
        .prefix("objectify-extractor-")
        .suffix(".py")
        .tempfile()?;
    write!(tmpfile, "{}", extractor)?;

    let out = std::process::Command::new(&python)
        .arg(tmpfile.path())
        .output();

    schema_from_output(out)
}

fn schema_from_output(
    out: std::io::Result<std::process::Output>,
) -> Result<Option<String>> {
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() || s == "null" { Ok(None) } else { Ok(Some(s)) }
        }
        _ => Ok(None),
    }
}

// ─── COMMANDS ────────────────────────────────────────────────────────────────

fn cmd_init(global: bool) -> Result<()> {
    let dir = if global {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow!("cannot determine home directory"))?;
        home.join(".objectify")
    } else {
        std::env::current_dir()?.join(".objectify")
    };

    if dir.exists() {
        bail!("{} already exists", dir.display());
    }

    std::fs::create_dir_all(dir.join("classes"))?;
    std::fs::write(
        dir.join("deno.json"),
        r#"{"imports": {"objectify": "npm:objectify"}}"#,
    )?;

    let conn = Connection::open(dir.join("objectify.db"))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    init_schema(&conn)?;

    print_json(&serde_json::json!({ "initialized": dir.display().to_string() }));
    Ok(())
}

fn cmd_create(
    ctx: &ObjectifyContext,
    description: Option<String>,
    class: Option<String>,
    expire: Option<String>,
) -> Result<()> {
    let conn = ctx.open_db()?;
    let id = new_id();
    let now = Utc::now().to_rfc3339();
    let expires_at = expire.as_deref().map(parse_expiry_duration).transpose()?;

    // Best-effort schema extraction when a class is specified
    let schema = if let Some(ref class_name) = class {
        extract_schema(ctx, class_name)?
    } else {
        None
    };

    conn.execute(
        "INSERT INTO objects (id, class, description, schema, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, class, description, schema, now, expires_at],
    )?;

    // Write the initial (empty) state event
    write_event(&conn, &id, "create", &Value::Null)?;

    print_json(&display_id(&conn, &id));
    Ok(())
}

fn cmd_destroy(ctx: &ObjectifyContext, prefix: &str) -> Result<()> {
    let conn = ctx.open_db()?;
    let id = resolve_id(&conn, prefix)?;
    conn.execute("DELETE FROM objects WHERE id = ?1", params![id])?;
    print_json(&serde_json::json!({ "destroyed": prefix }));
    Ok(())
}

fn cmd_inspect(ctx: &ObjectifyContext, prefix: &str) -> Result<()> {
    let conn = ctx.open_db()?;
    let id = resolve_id(&conn, prefix)?;
    let obj = get_object(&conn, &id)?;

    if is_expired(&obj) {
        let w = serde_json::json!({ "warning": format!("object {} is expired", display_id(&conn, &id)) });
        eprintln!("{}", serde_json::to_string(&w).unwrap());
    }

    let versions = version_count(&conn, &id)?;
    let sid = display_id(&conn, &id);

    print_json(&serde_json::json!({
        "id": obj.id,
        "shortId": sid,
        "class": obj.class,
        "description": obj.description,
        "versions": versions,
        "createdAt": obj.created_at,
        "expiresAt": obj.expires_at,
    }));
    Ok(())
}

fn cmd_list(
    ctx: &ObjectifyContext,
    class: Option<String>,
    include_expired: bool,
    since: Option<String>,
    limit: u64,
    offset: u64,
    json_output: bool,
) -> Result<()> {
    let conn = ctx.open_db()?;
    let since_dt = since.as_deref().map(parse_since_filter).transpose()?;
    let now_str = Utc::now().to_rfc3339();

    // Build dynamic query
    let mut where_parts: Vec<&str> = vec![];
    let mut params: Vec<String> = vec![];

    if !include_expired {
        where_parts.push("(o.expires_at IS NULL OR o.expires_at > ?)");
        params.push(now_str.clone());
    }

    if let Some(ref c) = class {
        where_parts.push("o.class = ?");
        params.push(c.clone());
    }

    if let Some(ref s) = since_dt {
        where_parts.push("o.created_at >= ?");
        params.push(s.clone());
    }

    let where_clause = if where_parts.is_empty() {
        "1=1".to_string()
    } else {
        where_parts.join(" AND ")
    };

    params.push(limit.to_string());
    params.push(offset.to_string());

    let sql = format!(
        "SELECT o.id, o.class, o.description, o.created_at, o.expires_at, \
         (SELECT COUNT(*) FROM events WHERE object_id = o.id) as versions \
         FROM objects o WHERE {where_clause} ORDER BY o.created_at DESC LIMIT ? OFFSET ?"
    );

    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<(String, Option<String>, Option<String>, String, Option<String>, i64)> = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;

    if json_output || !is_tty() {
        let objects: Vec<Value> = rows
            .iter()
            .map(|(id, class, desc, created_at, expires_at, versions)| {
                serde_json::json!({
                    "id": id,
                    "shortId": display_id(&conn, id),
                    "class": class,
                    "description": desc,
                    "versions": versions,
                    "createdAt": created_at,
                    "expiresAt": expires_at,
                })
            })
            .collect();
        print_json(&objects);
    } else {
        println!(
            "{:<8} {:<12} {:<22} {:<5} {:<16} {}",
            "ID", "CLASS", "DESCRIPTION", "VER", "CREATED", "EXPIRES"
        );
        for (id, class, desc, created_at, expires_at, versions) in &rows {
            let sid = display_id(&conn, id);
            let class_str = class.as_deref().unwrap_or("-");
            let desc_str = desc.as_deref().unwrap_or("-");
            let desc_truncated = if desc_str.len() > 20 {
                format!("{}…", &desc_str[..19])
            } else {
                desc_str.to_string()
            };
            println!(
                "{:<8} {:<12} {:<22} {:<5} {:<16} {}",
                sid,
                class_str,
                desc_truncated,
                versions,
                human_relative(created_at),
                human_expiry(expires_at.as_deref()),
            );
        }
    }
    Ok(())
}

/// Parse method input from trailing args after the method name.
///
/// Accepted forms (all equivalent for `add --parameter:title "write tests"`):
///   `'{"title": "write tests"}'`          — positional JSON object (existing)
///   `--parameter '{"title": "..."}'`      — named, full JSON
///   `-p '{"title": "..."}'`               — short, full JSON
///   `--parameter:title "write tests"`     — named, kv pair (value auto-parsed)
///   `-p:title "write tests"`              — short, kv pair
///
/// Multiple kv pairs are merged into a single object.
/// If both a positional JSON object and kv flags are present, the kv flags win.
fn parse_method_input(args: &[String]) -> Result<Option<Value>> {
    let mut kv: serde_json::Map<String, Value> = serde_json::Map::new();
    let mut positional: Option<Value> = None;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];

        // --parameter:key value  or  -p:key value
        if let Some(rest) = arg
            .strip_prefix("--parameter:")
            .or_else(|| arg.strip_prefix("-p:"))
        {
            let key = rest.to_string();
            i += 1;
            let raw = args
                .get(i)
                .ok_or_else(|| anyhow!("--parameter:{} requires a value", key))?;
            // Try to parse as JSON first; fall back to bare string.
            let val: Value = serde_json::from_str(raw)
                .unwrap_or_else(|_| Value::String(raw.clone()));
            kv.insert(key, val);
            i += 1;
            continue;
        }

        // --parameter <json>  or  -p <json>
        if arg == "--parameter" || arg == "-p" {
            i += 1;
            let raw = args
                .get(i)
                .ok_or_else(|| anyhow!("{} requires a JSON argument", arg))?;
            let val: Value = serde_json::from_str(raw)
                .map_err(|e| anyhow!("invalid JSON after {}: {}", arg, e))?;
            // If it's an object, merge keys into kv accumulator.
            if let Value::Object(map) = val {
                kv.extend(map);
            } else {
                // Non-object JSON passed directly — return immediately.
                return Ok(Some(val));
            }
            i += 1;
            continue;
        }

        // Positional argument — treat as JSON input (existing behaviour).
        if positional.is_none() && !arg.starts_with('-') {
            positional = Some(
                serde_json::from_str(arg)
                    .map_err(|e| anyhow!("invalid JSON input: {}", e))?,
            );
        }

        i += 1;
    }

    if !kv.is_empty() {
        Ok(Some(Value::Object(kv)))
    } else {
        Ok(positional)
    }
}

fn cmd_use(ctx: &ObjectifyContext, prefix: &str, args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("usage: objectify use <id> <get|set <json>|<method> [input]>");
    }

    let conn = ctx.open_db()?;
    let id = resolve_id(&conn, prefix)?;
    let obj = get_object(&conn, &id)?;

    if is_expired(&obj) {
        let w = serde_json::json!({
            "warning": format!("object {} is expired", display_id(&conn, &id))
        });
        eprintln!("{}", serde_json::to_string(&w).unwrap());
    }

    let subcommand = args[0].as_str();

    match subcommand {
        "get" => {
            // Parse optional --at=<version>
            let at_version = args
                .iter()
                .find(|a| a.starts_with("--at="))
                .and_then(|a| a.strip_prefix("--at="))
                .and_then(|v| v.parse::<u64>().ok());
            let state = get_state_at(&conn, &id, at_version)?;
            print_json(&state);
        }
        "set" => {
            let json_str = args
                .get(1)
                .ok_or_else(|| anyhow!("usage: objectify use <id> set <json>"))?;
            let state: Value = serde_json::from_str(json_str)
                .map_err(|e| anyhow!("invalid JSON: {}", e))?;
            if let Some(ref schema_str) = obj.schema {
                validate_state(schema_str, &state)?;
            }
            write_event(&conn, &id, "set", &state)?;
            print_json(&serde_json::json!({ "ok": true }));
        }
        "help" => {
            let class_name = obj.class.as_ref().ok_or_else(|| {
                anyhow!("object has no class; 'help' is only available for typed objects")
            })?;
            cmd_describe_class(ctx, class_name, false)?;
        }
        method => {
            let class_name = obj.class.as_ref().ok_or_else(|| {
                anyhow!(
                    "object has no class; cannot call method '{}'. \
                     Use 'get' or 'set' for untyped objects.",
                    method
                )
            })?;
            let input: Option<Value> = parse_method_input(&args[1..])?;
            let current_state = get_state_at(&conn, &id, None).unwrap_or(Value::Null);
            let result =
                run_class_method(ctx, class_name, method, &current_state, input.as_ref())?;

            if result.state_changed {
                if let Some(ref new_state) = result.state {
                    if let Some(ref schema_str) = obj.schema {
                        validate_state(schema_str, new_state)?;
                    }
                    write_event(&conn, &id, method, new_state)?;
                }
            }

            print_json(&result.result);
        }
    }
    Ok(())
}

fn cmd_log(ctx: &ObjectifyContext, prefix: &str) -> Result<()> {
    let conn = ctx.open_db()?;
    let id = resolve_id(&conn, prefix)?;

    let mut stmt = conn.prepare(
        "SELECT version, method, created_at FROM events WHERE object_id = ?1 ORDER BY version ASC",
    )?;
    let rows: Vec<(i64, String, String)> = stmt
        .query_map(params![id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;

    if !is_tty() {
        let log: Vec<Value> = rows
            .iter()
            .map(|(v, m, at)| serde_json::json!({ "version": v, "method": m, "at": at }))
            .collect();
        print_json(&log);
    } else {
        println!("{:<8} {:<14} {}", "VERSION", "METHOD", "AT");
        for (version, method, created_at) in &rows {
            println!("{:<8} {:<14} {}", version, method, human_relative(created_at));
        }
    }
    Ok(())
}

fn cmd_diff(ctx: &ObjectifyContext, prefix: &str, v1: u64, v2: u64) -> Result<()> {
    let conn = ctx.open_db()?;
    let id = resolve_id(&conn, prefix)?;
    let state1 = get_state_at(&conn, &id, Some(v1))?;
    let state2 = get_state_at(&conn, &id, Some(v2))?;
    let patch = json_patch::diff(&state1, &state2);
    print_json(&patch);
    Ok(())
}

fn cmd_rewind(ctx: &ObjectifyContext, prefix: &str, version: u64) -> Result<()> {
    let conn = ctx.open_db()?;
    let id = resolve_id(&conn, prefix)?;
    let state = get_state_at(&conn, &id, Some(version))?;
    let new_version = write_event(&conn, &id, "rewind", &state)?;
    print_json(&serde_json::json!({
        "rewoundTo": version,
        "newVersion": new_version,
    }));
    Ok(())
}

fn cmd_fork(ctx: &ObjectifyContext, prefix: &str, at: Option<u64>) -> Result<()> {
    let conn = ctx.open_db()?;
    let id = resolve_id(&conn, prefix)?;
    let obj = get_object(&conn, &id)?;
    let state = get_state_at(&conn, &id, at)?;

    let new_id = new_id();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO objects (id, class, description, schema, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![new_id, obj.class, obj.description, obj.schema, now, obj.expires_at],
    )?;
    write_event(&conn, &new_id, "create", &state)?;

    print_json(&display_id(&conn, &new_id));
    Ok(())
}

fn cmd_gc(ctx: &ObjectifyContext) -> Result<()> {
    let conn = ctx.open_db()?;
    let now = Utc::now().to_rfc3339();
    let deleted = conn.execute(
        "DELETE FROM objects WHERE expires_at IS NOT NULL AND expires_at <= ?1",
        params![now],
    )?;
    print_json(&serde_json::json!({ "deleted": deleted }));
    Ok(())
}

// ─── CLASS INTROSPECTION ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct MethodInfo {
    name: String,
    params: String,
    #[serde(rename = "returnType")]
    return_type: Option<String>,
    #[serde(rename = "async")]
    is_async: bool,
}

fn generate_ts_introspector(class_file: &Path) -> String {
    let class_path = class_file.display();
    format!(
        r#"import UserClass from '{class_path}';

const instance = new UserClass();
(instance as any).get = async () => ({{}});
(instance as any).set = async () => {{}};

const methods: any[] = [];
const skip = new Set(['get', 'set', 'constructor']);

// Arrow function class fields (own properties)
for (const key of Object.getOwnPropertyNames(instance)) {{
  if (skip.has(key) || key.startsWith('_')) continue;
  const val = (instance as any)[key];
  if (typeof val !== 'function') continue;
  skip.add(key);
  const src = val.toString();
  // Extract params from arrow function source
  const match = src.match(/^\s*(?:async\s*)?\(([^)]*)\)/);
  const params = match ? match[1].trim() : '';
  // Extract return type from arrow: ) : Type => or ): Type =>
  const retMatch = src.match(/\)\s*:\s*([^=]+?)\s*=>/);
  const returnType = retMatch ? retMatch[1].trim() : null;
  methods.push({{
    name: key,
    params: params || '()',
    returnType,
    async: val.constructor.name === 'AsyncFunction',
  }});
}}

// Prototype methods
for (const key of Object.getOwnPropertyNames(Object.getPrototypeOf(instance))) {{
  if (skip.has(key) || key.startsWith('_')) continue;
  const val = (instance as any)[key];
  if (typeof val !== 'function') continue;
  const src = val.toString();
  const match = src.match(/^\s*(?:async\s+)?\w+\s*\(([^)]*)\)/);
  const params = match ? match[1].trim() : '';
  methods.push({{
    name: key,
    params: params || '()',
    returnType: null,
    async: val.constructor.name === 'AsyncFunction',
  }});
}}

console.log(JSON.stringify(methods));
"#
    )
}

fn generate_py_introspector(class_file: &Path, class_name: &str) -> String {
    let class_path = class_file.display();
    format!(
        r#"import sys, json, types, importlib.util, inspect

class _DoBase:
    def __class_getitem__(cls, _item): return cls
    async def get(self): return {{}}
    async def set(self, state): pass

_mod = types.ModuleType('objectify')
_mod.DoBase = _DoBase
sys.modules['objectify'] = _mod

_spec = importlib.util.spec_from_file_location("_uc", "{class_path}")
_module = importlib.util.module_from_spec(_spec)
_module.__package__ = ""
_spec.loader.exec_module(_module)
UserClass = getattr(_module, "{class_name}", None)
if UserClass is None:
    for _name, _obj in inspect.getmembers(_module, inspect.isclass):
        if _name != '_DoBase' and issubclass(_obj, _DoBase) and _obj is not _DoBase:
            UserClass = _obj
            break
if UserClass is None:
    print("[]"); sys.exit(0)

_skip = {{'get', 'set'}}
methods = []
instance = UserClass()
for name in sorted(dir(instance)):
    if name.startswith('_') or name in _skip:
        continue
    method = getattr(instance, name, None)
    if not callable(method):
        continue
    try:
        sig = inspect.signature(method)
    except (ValueError, TypeError):
        sig = None
    params_str = str(sig) if sig else '()'
    ret = None
    if sig and sig.return_annotation is not inspect.Signature.empty:
        ret = str(sig.return_annotation)
    is_async = inspect.iscoroutinefunction(method)
    methods.append({{
        "name": name,
        "params": params_str,
        "returnType": ret,
        "async": is_async,
    }})

print(json.dumps(methods))
"#,
        class_path = class_path,
        class_name = class_name,
    )
}

fn extract_methods(ctx: &ObjectifyContext, class_name: &str) -> Result<Vec<MethodInfo>> {
    let (class_file, lang) = find_class_file(ctx, class_name)?;

    use std::io::Write as IoWrite;

    match lang {
        ClassLang::TypeScript => {
            let deno = find_deno()?;
            let script = generate_ts_introspector(&class_file);
            let mut tmpfile = tempfile::Builder::new()
                .prefix("objectify-introspect-")
                .suffix(".ts")
                .tempfile()?;
            write!(tmpfile, "{}", script)?;

            let flags = build_deno_flags(ctx, &Value::Null);
            let mut cmd = std::process::Command::new(&deno);
            cmd.arg("run");
            for flag in &flags {
                cmd.arg(flag);
            }
            cmd.arg(tmpfile.path());

            let output = cmd.output()?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("introspection failed:\n{}", stderr);
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            let methods: Vec<MethodInfo> = serde_json::from_str(&stdout)
                .map_err(|e| anyhow!("failed to parse introspection output: {}", e))?;
            Ok(methods)
        }
        ClassLang::Python => {
            let python = find_python()?;
            let script = generate_py_introspector(&class_file, class_name);
            let mut tmpfile = tempfile::Builder::new()
                .prefix("objectify-introspect-")
                .suffix(".py")
                .tempfile()?;
            write!(tmpfile, "{}", script)?;

            let output = std::process::Command::new(&python)
                .arg(tmpfile.path())
                .output()?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("introspection failed:\n{}", stderr);
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            let methods: Vec<MethodInfo> = serde_json::from_str(&stdout)
                .map_err(|e| anyhow!("failed to parse introspection output: {}", e))?;
            Ok(methods)
        }
    }
}

fn cmd_describe_class(ctx: &ObjectifyContext, class_name: &str, force_json: bool) -> Result<()> {
    let (_, lang) = find_class_file(ctx, class_name)?;
    let methods = extract_methods(ctx, class_name)?;

    let lang_str = match lang {
        ClassLang::TypeScript => "TypeScript",
        ClassLang::Python => "Python",
    };

    if force_json || !is_tty() {
        let output = serde_json::json!({
            "class": class_name,
            "lang": lang_str,
            "methods": methods,
        });
        print_json(&output);
    } else {
        println!("Class: {} ({})\n", class_name, lang_str);
        if methods.is_empty() {
            println!("  No methods found.");
        } else {
            // Calculate column widths
            let name_w = methods.iter().map(|m| m.name.len()).max().unwrap_or(6).max(6);
            let params_w = methods.iter().map(|m| m.params.len()).max().unwrap_or(10).max(10);
            println!(
                "  {:<name_w$}  {:<params_w$}  {:<10}  {}",
                "METHOD", "PARAMETERS", "RETURNS", "ASYNC",
                name_w = name_w,
                params_w = params_w,
            );
            for m in &methods {
                let ret = m.return_type.as_deref().unwrap_or("-");
                let async_str = if m.is_async { "yes" } else { "no" };
                println!(
                    "  {:<name_w$}  {:<params_w$}  {:<10}  {}",
                    m.name, m.params, ret, async_str,
                    name_w = name_w,
                    params_w = params_w,
                );
            }
        }
    }
    Ok(())
}

fn cmd_classes(ctx: &ObjectifyContext, force_json: bool) -> Result<()> {
    let classes_dir = &ctx.classes_dir;
    if !classes_dir.exists() {
        if force_json || !is_tty() {
            print_json(&serde_json::json!([]));
        } else {
            println!("No classes directory found at {}", classes_dir.display());
        }
        return Ok(());
    }

    #[derive(Serialize)]
    struct ClassEntry {
        name: String,
        lang: String,
        file: String,
        objects: u64,
    }

    let conn = ctx.open_db()?;

    let mut entries: Vec<ClassEntry> = Vec::new();
    let mut dir_entries: Vec<_> = std::fs::read_dir(classes_dir)?
        .filter_map(|e| e.ok())
        .collect();
    dir_entries.sort_by_key(|e| e.file_name());

    for entry in dir_entries {
        let path = entry.path();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let lang = match ext {
            "ts" => "TypeScript",
            "py" => "Python",
            _ => continue,
        };
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let objects: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM objects WHERE class = ?1",
                params![name],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as u64;
        entries.push(ClassEntry {
            name,
            lang: lang.to_string(),
            file: path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string(),
            objects,
        });
    }

    if force_json || !is_tty() {
        print_json(&entries);
    } else {
        if entries.is_empty() {
            println!("No classes found in {}", classes_dir.display());
            return Ok(());
        }
        println!("{:<20} {:<12} {:<8} {}", "NAME", "LANG", "OBJECTS", "FILE");
        for e in &entries {
            println!("{:<20} {:<12} {:<8} {}", e.name, e.lang, e.objects, e.file);
        }
    }
    Ok(())
}

// ─── SKILL INSTALL ───────────────────────────────────────────────────────────

/// SKILL.md content embedded at compile time.
const SKILL_MD: &str = include_str!("../SKILL.md");

fn find_skills_dir() -> Result<PathBuf> {
    // Walk up from cwd looking for .skills/skills/
    let mut d = std::env::current_dir()?;
    loop {
        let candidate = d.join(".skills").join("skills");
        if candidate.is_dir() {
            return Ok(candidate);
        }
        if !d.pop() {
            break;
        }
    }
    // Fall back to ~/.skills/skills/
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("cannot determine home directory"))?;
    Ok(home.join(".skills").join("skills"))
}

fn cmd_skill_install(global: bool, path: Option<PathBuf>) -> Result<()> {
    let skills_dir = if let Some(p) = path {
        p
    } else if global {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow!("cannot determine home directory"))?;
        home.join(".skills").join("skills")
    } else {
        find_skills_dir()?
    };

    let target_dir = skills_dir.join("objectify");
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| anyhow!("failed to create {}: {}", target_dir.display(), e))?;

    let skill_file = target_dir.join("SKILL.md");
    std::fs::write(&skill_file, SKILL_MD)
        .map_err(|e| anyhow!("failed to write {}: {}", skill_file.display(), e))?;

    print_json(&serde_json::json!({
        "installed": skill_file.to_string_lossy(),
    }));
    Ok(())
}

// ─── MAIN ────────────────────────────────────────────────────────────────────

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init { global } => cmd_init(global),
        Command::Create { description, class, expire } => {
            let ctx = ObjectifyContext::resolve()?;
            cmd_create(&ctx, description, class, expire)
        }
        Command::Destroy { id } => {
            let ctx = ObjectifyContext::resolve()?;
            cmd_destroy(&ctx, &id)
        }
        Command::Inspect { id } => {
            let ctx = ObjectifyContext::resolve()?;
            cmd_inspect(&ctx, &id)
        }
        Command::List { class, expired, since, limit, offset, json } => {
            let ctx = ObjectifyContext::resolve()?;
            cmd_list(&ctx, class, expired, since, limit, offset, json)
        }
        Command::Use { id, args } => {
            let ctx = ObjectifyContext::resolve()?;
            cmd_use(&ctx, &id, args)
        }
        Command::Log { id } => {
            let ctx = ObjectifyContext::resolve()?;
            cmd_log(&ctx, &id)
        }
        Command::Diff { id, v1, v2 } => {
            let ctx = ObjectifyContext::resolve()?;
            cmd_diff(&ctx, &id, v1, v2)
        }
        Command::Rewind { id, version } => {
            let ctx = ObjectifyContext::resolve()?;
            cmd_rewind(&ctx, &id, version)
        }
        Command::Fork { id, at } => {
            let ctx = ObjectifyContext::resolve()?;
            cmd_fork(&ctx, &id, at)
        }
        Command::Gc => {
            let ctx = ObjectifyContext::resolve()?;
            cmd_gc(&ctx)
        }
        Command::Classes { name, json } => {
            let ctx = ObjectifyContext::resolve()?;
            match name {
                Some(class_name) => cmd_describe_class(&ctx, &class_name, json),
                None => cmd_classes(&ctx, json),
            }
        }
        Command::Skill { action } => match action {
            SkillAction::Install { global, path } => cmd_skill_install(global, path),
        },
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("{}", serde_json::json!({ "error": e.to_string() }));
        std::process::exit(1);
    }
}

// ─── TESTS ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::json;
    use tempfile::TempDir;

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Open an in-memory SQLite database with the objectify schema initialised.
    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    /// Insert an object row and a "create" event, returning the full ID.
    fn insert_obj(conn: &Connection, class: Option<&str>, description: Option<&str>) -> String {
        let id = new_id();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO objects (id, class, description, schema, created_at, expires_at) VALUES (?1, ?2, ?3, NULL, ?4, NULL)",
            params![id, class, description, now],
        ).unwrap();
        write_event(conn, &id, "create", &json!(null)).unwrap();
        id
    }

    /// Shorthand to build a `Vec<String>` from string literals.
    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // ── init_schema ───────────────────────────────────────────────────────────

    #[test]
    fn schema_is_idempotent() {
        // Calling init_schema twice on the same DB must not fail.
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_schema(&conn).unwrap();
        // Both tables must exist.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('objects','events')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    // ── new_id / resolve_id / display_id ─────────────────────────────────────

    #[test]
    fn new_id_is_32_char_lowercase_hex() {
        let id = new_id();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn resolve_id_exact_match() {
        let conn = mem_db();
        let id = insert_obj(&conn, None, None);
        let resolved = resolve_id(&conn, &id).unwrap();
        assert_eq!(resolved, id);
    }

    #[test]
    fn resolve_id_prefix_match() {
        let conn = mem_db();
        let id = insert_obj(&conn, None, None);
        // 4-char prefix (minimum shown in output)
        let prefix = &id[..4];
        let resolved = resolve_id(&conn, prefix).unwrap();
        assert_eq!(resolved, id);
    }

    #[test]
    fn resolve_id_case_insensitive() {
        let conn = mem_db();
        let id = insert_obj(&conn, None, None);
        let lower_prefix = id[..4].to_lowercase();
        let resolved = resolve_id(&conn, &lower_prefix).unwrap();
        assert_eq!(resolved, id);
    }

    #[test]
    fn resolve_id_not_found() {
        let conn = mem_db();
        let err = resolve_id(&conn, "ffffffff").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn resolve_id_ambiguous() {
        let conn = mem_db();
        // Force two objects with identical first 4 chars — achieved by inserting known IDs directly.
        let id_a = "aabb0000000000000000000000000000".to_string();
        let id_b = "aabb1111111111111111111111111111".to_string();
        let now = Utc::now().to_rfc3339();
        for id in [&id_a, &id_b] {
            conn.execute(
                "INSERT INTO objects (id, class, description, schema, created_at, expires_at) VALUES (?1, NULL, NULL, NULL, ?2, NULL)",
                params![id, now],
            ).unwrap();
        }
        let err = resolve_id(&conn, "aabb").unwrap_err();
        assert!(err.to_string().contains("ambiguous"));
    }

    #[test]
    fn display_id_minimum_four_chars() {
        let conn = mem_db();
        let id = insert_obj(&conn, None, None);
        let short = display_id(&conn, &id);
        assert!(short.len() >= 4);
        // Must be a prefix of the original id (case-insensitive).
        assert!(id.to_lowercase().starts_with(&short));
    }

    // ── parse_expiry_duration ─────────────────────────────────────────────────

    #[test]
    fn expiry_seconds() {
        let result = parse_expiry_duration("30s").unwrap();
        let dt = DateTime::parse_from_rfc3339(&result).unwrap();
        let diff = dt.with_timezone(&Utc).signed_duration_since(Utc::now()).num_seconds();
        assert!((28..=32).contains(&diff), "expected ~30s, got {}s", diff);
    }

    #[test]
    fn expiry_minutes() {
        let result = parse_expiry_duration("5m").unwrap();
        let dt = DateTime::parse_from_rfc3339(&result).unwrap();
        let diff = dt.with_timezone(&Utc).signed_duration_since(Utc::now()).num_seconds();
        assert!((298..=302).contains(&diff));
    }

    #[test]
    fn expiry_hours() {
        let result = parse_expiry_duration("2h").unwrap();
        let dt = DateTime::parse_from_rfc3339(&result).unwrap();
        let diff = dt.with_timezone(&Utc).signed_duration_since(Utc::now()).num_seconds();
        assert!((7198..=7202).contains(&diff));
    }

    #[test]
    fn expiry_days() {
        let result = parse_expiry_duration("7d").unwrap();
        let dt = DateTime::parse_from_rfc3339(&result).unwrap();
        let diff = dt.with_timezone(&Utc).signed_duration_since(Utc::now()).num_seconds();
        let expected = 7 * 86_400i64;
        assert!((expected - 2..=expected + 2).contains(&diff));
    }

    #[test]
    fn expiry_weeks() {
        let result = parse_expiry_duration("2w").unwrap();
        let dt = DateTime::parse_from_rfc3339(&result).unwrap();
        let diff = dt.with_timezone(&Utc).signed_duration_since(Utc::now()).num_seconds();
        let expected = 14 * 86_400i64;
        assert!((expected - 2..=expected + 2).contains(&diff));
    }

    #[test]
    fn expiry_invalid_unit() {
        let err = parse_expiry_duration("5x").unwrap_err();
        assert!(err.to_string().contains("invalid expiry unit"));
    }

    #[test]
    fn expiry_invalid_number() {
        let err = parse_expiry_duration("abcd").unwrap_err();
        assert!(err.to_string().contains("invalid expiry"));
    }

    #[test]
    fn expiry_empty_string() {
        let err = parse_expiry_duration("").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    // ── parse_since_filter ────────────────────────────────────────────────────

    #[test]
    fn since_relative_duration() {
        let result = parse_since_filter("1h").unwrap();
        let dt = DateTime::parse_from_rfc3339(&result).unwrap();
        let diff = Utc::now().signed_duration_since(dt.with_timezone(&Utc)).num_seconds();
        assert!((3598..=3602).contains(&diff));
    }

    #[test]
    fn since_iso_datetime() {
        let input = "2025-01-01T00:00:00Z";
        let result = parse_since_filter(input).unwrap();
        assert!(result.starts_with("2025-01-01"));
    }

    #[test]
    fn since_date_only() {
        let result = parse_since_filter("2025-06-15").unwrap();
        assert!(result.starts_with("2025-06-15"));
    }

    #[test]
    fn since_invalid() {
        let err = parse_since_filter("not-a-date").unwrap_err();
        assert!(err.to_string().contains("invalid date"));
    }

    // ── is_expired ────────────────────────────────────────────────────────────

    #[test]
    fn not_expired_when_expires_at_is_none() {
        let obj = ObjectRow {
            id: "x".into(),
            class: None,
            description: None,
            schema: None,
            created_at: Utc::now().to_rfc3339(),
            expires_at: None,
        };
        assert!(!is_expired(&obj));
    }

    #[test]
    fn not_expired_when_future() {
        let future = (Utc::now() + Duration::hours(1)).to_rfc3339();
        let obj = ObjectRow {
            id: "x".into(),
            class: None,
            description: None,
            schema: None,
            created_at: Utc::now().to_rfc3339(),
            expires_at: Some(future),
        };
        assert!(!is_expired(&obj));
    }

    #[test]
    fn expired_when_past() {
        let past = (Utc::now() - Duration::seconds(1)).to_rfc3339();
        let obj = ObjectRow {
            id: "x".into(),
            class: None,
            description: None,
            schema: None,
            created_at: Utc::now().to_rfc3339(),
            expires_at: Some(past),
        };
        assert!(is_expired(&obj));
    }

    // ── write_event / get_state_at / next_version / version_count ─────────────

    #[test]
    fn write_and_read_state() {
        let conn = mem_db();
        let id = insert_obj(&conn, None, None);
        let state = json!({"count": 1});
        write_event(&conn, &id, "set", &state).unwrap();
        let latest = get_state_at(&conn, &id, None).unwrap();
        // The last event is version 2 (version 1 = "create" null, version 2 = "set")
        assert_eq!(latest, state);
    }

    #[test]
    fn get_state_at_specific_version() {
        let conn = mem_db();
        let id = insert_obj(&conn, None, None);
        write_event(&conn, &id, "set", &json!({"v": 1})).unwrap();
        write_event(&conn, &id, "set", &json!({"v": 2})).unwrap();
        let v1 = get_state_at(&conn, &id, Some(2)).unwrap();
        assert_eq!(v1["v"], 1);
        let v2 = get_state_at(&conn, &id, Some(3)).unwrap();
        assert_eq!(v2["v"], 2);
    }

    #[test]
    fn version_count_increments() {
        let conn = mem_db();
        let id = insert_obj(&conn, None, None); // writes 1 event (create)
        assert_eq!(version_count(&conn, &id).unwrap(), 1);
        write_event(&conn, &id, "set", &json!({})).unwrap();
        assert_eq!(version_count(&conn, &id).unwrap(), 2);
        write_event(&conn, &id, "set", &json!({})).unwrap();
        assert_eq!(version_count(&conn, &id).unwrap(), 3);
    }

    #[test]
    fn next_version_starts_at_one_for_new_object() {
        let conn = mem_db();
        let id = new_id();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO objects (id, class, description, schema, created_at, expires_at) VALUES (?1, NULL, NULL, NULL, ?2, NULL)",
            params![id, now],
        ).unwrap();
        assert_eq!(next_version(&conn, &id).unwrap(), 1);
    }

    // ── validate_state ────────────────────────────────────────────────────────

    #[test]
    fn validates_against_schema_ok() {
        let schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}"#;
        let state = json!({"name": "alice"});
        validate_state(schema, &state).unwrap();
    }

    #[test]
    fn validates_against_schema_fail() {
        let schema = r#"{"type": "object", "properties": {"age": {"type": "integer"}}, "required": ["age"]}"#;
        let state = json!({"age": "not-a-number"});
        let err = validate_state(schema, &state).unwrap_err();
        assert!(err.to_string().contains("validation failed"));
    }

    #[test]
    fn validate_rejects_wrong_type() {
        let schema = r#"{"type": "array"}"#;
        let state = json!({"key": "value"});
        let err = validate_state(schema, &state).unwrap_err();
        assert!(err.to_string().contains("validation failed"));
    }

    #[test]
    fn validate_rejects_missing_required_field() {
        let schema = r#"{"type": "object", "required": ["id", "title"]}"#;
        let state = json!({"id": "abc"});
        let err = validate_state(schema, &state).unwrap_err();
        assert!(err.to_string().contains("validation failed"));
    }

    // ── parse_method_input ────────────────────────────────────────────────────

    #[test]
    fn no_args_returns_none() {
        let result = parse_method_input(&args(&[])).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn positional_json_object() {
        let result = parse_method_input(&args(&[r#"{"title":"test"}"#])).unwrap();
        assert_eq!(result.unwrap(), json!({"title": "test"}));
    }

    #[test]
    fn positional_json_scalar() {
        let result = parse_method_input(&args(&["42"])).unwrap();
        assert_eq!(result.unwrap(), json!(42));
    }

    #[test]
    fn positional_json_array() {
        let result = parse_method_input(&args(&[r#"[1,2,3]"#])).unwrap();
        assert_eq!(result.unwrap(), json!([1, 2, 3]));
    }

    #[test]
    fn short_flag_full_json() {
        let result = parse_method_input(&args(&["-p", r#"{"title":"test"}"#])).unwrap();
        assert_eq!(result.unwrap(), json!({"title": "test"}));
    }

    #[test]
    fn long_flag_full_json() {
        let result = parse_method_input(&args(&["--parameter", r#"{"title":"test"}"#])).unwrap();
        assert_eq!(result.unwrap(), json!({"title": "test"}));
    }

    #[test]
    fn short_flag_kv_string_value() {
        let result = parse_method_input(&args(&["-p:title", "write tests"])).unwrap();
        assert_eq!(result.unwrap(), json!({"title": "write tests"}));
    }

    #[test]
    fn long_flag_kv_string_value() {
        let result = parse_method_input(&args(&["--parameter:title", "write tests"])).unwrap();
        assert_eq!(result.unwrap(), json!({"title": "write tests"}));
    }

    #[test]
    fn kv_value_json_parsed_when_valid() {
        // Integer value should be stored as a number, not a string.
        let result = parse_method_input(&args(&["-p:priority", "3"])).unwrap();
        assert_eq!(result.unwrap(), json!({"priority": 3}));
    }

    #[test]
    fn kv_value_json_parsed_boolean() {
        let result = parse_method_input(&args(&["-p:done", "true"])).unwrap();
        assert_eq!(result.unwrap(), json!({"done": true}));
    }

    #[test]
    fn kv_value_falls_back_to_string_when_not_json() {
        let result = parse_method_input(&args(&["-p:label", "hello world"])).unwrap();
        assert_eq!(result.unwrap(), json!({"label": "hello world"}));
    }

    #[test]
    fn multiple_kv_pairs_merged() {
        let result = parse_method_input(&args(&[
            "-p:title", "write tests",
            "-p:priority", "1",
            "-p:done", "false",
        ])).unwrap();
        assert_eq!(result.unwrap(), json!({"title": "write tests", "priority": 1, "done": false}));
    }

    #[test]
    fn flag_full_json_object_merged_with_kv() {
        // --parameter <obj> whose keys get merged into the kv accumulator
        let result = parse_method_input(&args(&[
            "--parameter", r#"{"a":1}"#,
            "-p:b", "2",
        ])).unwrap();
        let v = result.unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], 2);
    }

    #[test]
    fn flag_full_json_non_object_scalar() {
        // A scalar passed via -p is returned directly without merging.
        let result = parse_method_input(&args(&["-p", "42"])).unwrap();
        assert_eq!(result.unwrap(), json!(42));
    }

    #[test]
    fn kv_flag_without_value_errors() {
        let err = parse_method_input(&args(&["-p:key"])).unwrap_err();
        assert!(err.to_string().contains("requires a value"));
    }

    #[test]
    fn short_flag_without_value_errors() {
        let err = parse_method_input(&args(&["-p"])).unwrap_err();
        assert!(err.to_string().contains("requires a JSON argument"));
    }

    #[test]
    fn long_flag_without_value_errors() {
        let err = parse_method_input(&args(&["--parameter"])).unwrap_err();
        assert!(err.to_string().contains("requires a JSON argument"));
    }

    #[test]
    fn flag_with_invalid_json_errors() {
        let err = parse_method_input(&args(&["-p", "not-json{"])).unwrap_err();
        assert!(err.to_string().contains("invalid JSON"));
    }

    // ── human_relative ────────────────────────────────────────────────────────

    #[test]
    fn human_relative_seconds() {
        let ts = (Utc::now() - Duration::seconds(10)).to_rfc3339();
        let s = human_relative(&ts);
        assert!(s.contains("secs ago"), "got: {}", s);
    }

    #[test]
    fn human_relative_minutes() {
        let ts = (Utc::now() - Duration::minutes(5)).to_rfc3339();
        let s = human_relative(&ts);
        assert!(s.contains("mins ago"), "got: {}", s);
    }

    #[test]
    fn human_relative_hours() {
        let ts = (Utc::now() - Duration::hours(3)).to_rfc3339();
        let s = human_relative(&ts);
        assert!(s.contains("hours ago"), "got: {}", s);
    }

    #[test]
    fn human_relative_days() {
        let ts = (Utc::now() - Duration::days(2)).to_rfc3339();
        let s = human_relative(&ts);
        assert!(s.contains("days ago"), "got: {}", s);
    }

    #[test]
    fn human_relative_weeks() {
        let ts = (Utc::now() - Duration::weeks(2)).to_rfc3339();
        let s = human_relative(&ts);
        assert!(s.contains("weeks ago"), "got: {}", s);
    }

    #[test]
    fn human_relative_future_returns_just_now() {
        let ts = (Utc::now() + Duration::seconds(5)).to_rfc3339();
        let s = human_relative(&ts);
        assert_eq!(s, "just now");
    }

    #[test]
    fn human_relative_old_shows_date() {
        let ts = (Utc::now() - Duration::days(60)).to_rfc3339();
        let s = human_relative(&ts);
        // Should be a date string like "2025-01-10"
        assert!(s.contains('-') && s.len() == 10, "got: {}", s);
    }

    // ── human_expiry ──────────────────────────────────────────────────────────

    #[test]
    fn human_expiry_none_is_never() {
        assert_eq!(human_expiry(None), "never");
    }

    #[test]
    fn human_expiry_past_is_expired() {
        let past = (Utc::now() - Duration::seconds(10)).to_rfc3339();
        assert_eq!(human_expiry(Some(&past)), "expired");
    }

    #[test]
    fn human_expiry_future_minutes() {
        let future = (Utc::now() + Duration::minutes(10)).to_rfc3339();
        let s = human_expiry(Some(&future));
        assert!(s.starts_with("in ") && s.contains("mins"), "got: {}", s);
    }

    #[test]
    fn human_expiry_future_hours() {
        let future = (Utc::now() + Duration::hours(2)).to_rfc3339();
        let s = human_expiry(Some(&future));
        assert!(s.starts_with("in ") && s.contains("hours"), "got: {}", s);
    }

    #[test]
    fn human_expiry_future_days() {
        let future = (Utc::now() + Duration::days(3)).to_rfc3339();
        let s = human_expiry(Some(&future));
        assert!(s.starts_with("in ") && s.contains("days"), "got: {}", s);
    }

    // ── DB integration: cascade delete ───────────────────────────────────────

    #[test]
    fn destroy_cascades_events() {
        let conn = mem_db();
        let id = insert_obj(&conn, None, None);
        write_event(&conn, &id, "set", &json!({"x": 1})).unwrap();
        assert_eq!(version_count(&conn, &id).unwrap(), 2);

        conn.execute("DELETE FROM objects WHERE id = ?1", params![id]).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events WHERE object_id = ?1", params![id], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "ON DELETE CASCADE must remove child events");
    }

    // ── DB integration: gc ────────────────────────────────────────────────────

    #[test]
    fn gc_removes_expired_objects() {
        let conn = mem_db();
        let past = (Utc::now() - Duration::seconds(1)).to_rfc3339();
        let future = (Utc::now() + Duration::hours(1)).to_rfc3339();

        // Expired object
        let exp_id = new_id();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO objects (id, class, description, schema, created_at, expires_at) VALUES (?1, NULL, NULL, NULL, ?2, ?3)",
            params![exp_id, now, past],
        ).unwrap();

        // Still-live object
        let live_id = new_id();
        conn.execute(
            "INSERT INTO objects (id, class, description, schema, created_at, expires_at) VALUES (?1, NULL, NULL, NULL, ?2, ?3)",
            params![live_id, now, future],
        ).unwrap();

        // Run GC manually (same logic as cmd_gc)
        let cutoff = Utc::now().to_rfc3339();
        let deleted = conn.execute(
            "DELETE FROM objects WHERE expires_at IS NOT NULL AND expires_at <= ?1",
            params![cutoff],
        ).unwrap();

        assert_eq!(deleted, 1);
        // Live object should still exist
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM objects WHERE id = ?1", params![live_id], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    // ── DB integration: rewind ────────────────────────────────────────────────

    #[test]
    fn rewind_writes_new_version_with_old_state() {
        let conn = mem_db();
        let id = insert_obj(&conn, None, None); // v1 = null
        write_event(&conn, &id, "set", &json!({"x": 1})).unwrap(); // v2
        write_event(&conn, &id, "set", &json!({"x": 2})).unwrap(); // v3

        // Rewind to v2
        let target_state = get_state_at(&conn, &id, Some(2)).unwrap();
        write_event(&conn, &id, "rewind", &target_state).unwrap(); // v4

        let latest = get_state_at(&conn, &id, None).unwrap();
        assert_eq!(latest, json!({"x": 1}));
        assert_eq!(version_count(&conn, &id).unwrap(), 4);
    }

    // ── DB integration: fork ──────────────────────────────────────────────────

    #[test]
    fn fork_produces_independent_object() {
        let conn = mem_db();
        let src = insert_obj(&conn, Some("MyClass"), Some("original"));
        write_event(&conn, &src, "set", &json!({"v": 42})).unwrap();

        // Fork manually (same logic as cmd_fork)
        let state = get_state_at(&conn, &src, None).unwrap();
        let fork_id = new_id();
        let now = Utc::now().to_rfc3339();
        let src_obj = get_object(&conn, &src).unwrap();
        conn.execute(
            "INSERT INTO objects (id, class, description, schema, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![fork_id, src_obj.class, src_obj.description, src_obj.schema, now, src_obj.expires_at],
        ).unwrap();
        write_event(&conn, &fork_id, "create", &state).unwrap();

        // Fork has same state
        let fork_state = get_state_at(&conn, &fork_id, None).unwrap();
        assert_eq!(fork_state, json!({"v": 42}));

        // Mutating the fork doesn't affect the source
        write_event(&conn, &fork_id, "set", &json!({"v": 99})).unwrap();
        let src_state = get_state_at(&conn, &src, None).unwrap();
        assert_eq!(src_state, json!({"v": 42}));
    }

    // ── find_class_file ───────────────────────────────────────────────────────

    #[test]
    fn find_class_file_ts_takes_precedence_over_py() {
        let dir = TempDir::new().unwrap();
        let classes = dir.path().join("classes");
        std::fs::create_dir_all(&classes).unwrap();
        std::fs::write(classes.join("Foo.ts"), "").unwrap();
        std::fs::write(classes.join("Foo.py"), "").unwrap();

        let ctx = ObjectifyContext {
            dir: dir.path().to_path_buf(),
            classes_dir: classes.clone(),
            db_path: dir.path().join("objectify.db"),
        };

        let (_path, lang) = find_class_file(&ctx, "Foo").unwrap();
        assert_eq!(lang, ClassLang::TypeScript);
    }

    #[test]
    fn find_class_file_falls_back_to_py() {
        let dir = TempDir::new().unwrap();
        let classes = dir.path().join("classes");
        std::fs::create_dir_all(&classes).unwrap();
        std::fs::write(classes.join("Bar.py"), "").unwrap();

        let ctx = ObjectifyContext {
            dir: dir.path().to_path_buf(),
            classes_dir: classes.clone(),
            db_path: dir.path().join("objectify.db"),
        };

        let (_path, lang) = find_class_file(&ctx, "Bar").unwrap();
        assert_eq!(lang, ClassLang::Python);
    }

    #[test]
    fn find_class_file_missing_errors() {
        let dir = TempDir::new().unwrap();
        let classes = dir.path().join("classes");
        std::fs::create_dir_all(&classes).unwrap();

        let ctx = ObjectifyContext {
            dir: dir.path().to_path_buf(),
            classes_dir: classes.clone(),
            db_path: dir.path().join("objectify.db"),
        };

        let err = find_class_file(&ctx, "NoSuch").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    // ── cmd_classes ───────────────────────────────────────────────────────────

    fn make_ctx_with_classes(dir: &TempDir, class_files: &[&str]) -> ObjectifyContext {
        let classes = dir.path().join("classes");
        std::fs::create_dir_all(&classes).unwrap();
        for f in class_files {
            std::fs::write(classes.join(f), "").unwrap();
        }
        ObjectifyContext {
            dir: dir.path().to_path_buf(),
            classes_dir: classes,
            db_path: dir.path().join("objectify.db"),
        }
    }

    #[test]
    fn classes_empty_directory() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx_with_classes(&dir, &[]);
        // Should succeed and not panic
        cmd_classes(&ctx, true).unwrap();
    }

    #[test]
    fn classes_lists_ts_and_py_files() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx_with_classes(&dir, &["TaskList.ts", "Counter.py", "Memory.ts"]);
        // Open the DB so object counts work
        let _conn = ctx.open_db().unwrap();
        // Should succeed without error
        cmd_classes(&ctx, true).unwrap();
    }

    #[test]
    fn classes_ignores_non_class_files() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx_with_classes(
            &dir,
            &["TaskList.ts", "README.md", "notes.txt", "Counter.py"],
        );
        let _conn = ctx.open_db().unwrap();
        // The command uses cmd_classes which filters by extension; other files are ignored.
        // We can't easily capture stdout in a unit test, but we verify it doesn't error.
        cmd_classes(&ctx, true).unwrap();
    }

    #[test]
    fn classes_counts_objects_per_class() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx_with_classes(&dir, &["TaskList.ts", "Counter.py"]);
        let conn = ctx.open_db().unwrap();

        // Insert two TaskList objects and one Counter object
        insert_obj(&conn, Some("TaskList"), None);
        insert_obj(&conn, Some("TaskList"), None);
        insert_obj(&conn, Some("Counter"), None);

        // Verify via direct SQL that our counts would be correct
        let tl_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM objects WHERE class = 'TaskList'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let ct_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM objects WHERE class = 'Counter'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tl_count, 2);
        assert_eq!(ct_count, 1);

        // And the command itself runs without error
        cmd_classes(&ctx, true).unwrap();
    }

    #[test]
    fn classes_no_classes_dir() {
        let dir = TempDir::new().unwrap();
        // Don't create the classes dir at all
        let ctx = ObjectifyContext {
            dir: dir.path().to_path_buf(),
            classes_dir: dir.path().join("classes"),
            db_path: dir.path().join("objectify.db"),
        };
        // Should succeed gracefully even with no directory
        cmd_classes(&ctx, true).unwrap();
    }
}
