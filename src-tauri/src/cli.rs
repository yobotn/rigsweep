//! Headless commands for scripts and AI agents. None of them needs administrator rights, so they
//! run before (and instead of) the elevated GUI:
//!
//!   rigsweep --disk-report [--json]           what Dev Disk would find, plus free space per drive
//!   rigsweep --disk-clean --safe [--dry-run] [--json]
//!                                             empty the caches that rebuild themselves (the same
//!                                             set as the weekly automatic cleanup)
//!   rigsweep --port <n> [--json]              which processes hold a local port
//!   rigsweep --mcp                            Model Context Protocol server on stdin/stdout
//!   rigsweep --help | --version
//!
//! Output goes to stdout (redirected, piped, or the console the command was started from).

use serde_json::{json, Value};
use std::io::{BufRead, Write};

use crate::{devdisk, hardware, netmon};

const FLAGS: &[&str] = &["--disk-report", "--disk-clean", "--port", "--mcp", "--help", "-h", "/?", "--version", "-V"];

const HELP: &str = "RigSweep: free up RAM, CPU and disk on Windows dev machines.
Run without arguments to open the app. Headless commands (no administrator rights needed):

  rigsweep --disk-report [--json]                  what Dev Disk would find, plus free space per drive
  rigsweep --disk-clean --safe [--dry-run] [--json]
                                                   empty the caches that rebuild themselves
  rigsweep --port <n> [--json]                     which processes hold a local port
  rigsweep --mcp                                   Model Context Protocol server on stdin/stdout
  rigsweep --help | --version

Exit codes: 0 ok, 2 bad usage. Docs: https://rigsweep.com/cli/";

/// Run a headless command if `args` asks for one. Returns false when the GUI should start.
pub fn try_run(args: &[String]) -> bool {
    let has = |f: &str| args.iter().any(|a| a.eq_ignore_ascii_case(f));
    if !FLAGS.iter().any(|f| has(f)) {
        return false;
    }
    attach_console();
    let json_out = has("--json");
    let code = if has("--help") || has("-h") || has("/?") {
        print!("{HELP}
");
        0
    } else if has("--version") || has("-V") {
        println!("rigsweep {}", env!("CARGO_PKG_VERSION"));
        0
    } else if has("--mcp") {
        mcp();
        0
    } else if has("--disk-report") {
        let r = disk_summary();
        if json_out { print_json(&r) } else { print_report(&r) }
        0
    } else if has("--disk-clean") {
        if !has("--safe") {
            eprintln!("--disk-clean needs --safe (only caches that rebuild themselves are cleaned from the command line)");
            2
        } else {
            let r = clean_safe(has("--dry-run"));
            if json_out { print_json(&r) } else { print_clean(&r) }
            0
        }
    } else {
        let port = args.iter().position(|a| a.eq_ignore_ascii_case("--port")).and_then(|i| args.get(i + 1)).and_then(|p| p.parse::<u16>().ok());
        match port {
            Some(p) => {
                let r = json!(netmon::port_owner(p));
                if json_out {
                    print_json(&r)
                } else if r.as_array().is_some_and(|a| a.is_empty()) {
                    println!("Nothing holds port {p}.");
                } else {
                    for o in r.as_array().into_iter().flatten() {
                        println!("{} (PID {}) {} {}  {}", o["name"].as_str().unwrap_or(""), o["pid"], o["proto"].as_str().unwrap_or(""),
                            o["states"].as_array().map(|s| s.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(",")).unwrap_or_default(),
                            o["cmd"].as_str().unwrap_or(""));
                    }
                }
                0
            }
            None => {
                eprintln!("usage: rigsweep --port <number>");
                2
            }
        }
    };
    let _ = std::io::stdout().flush();
    std::process::exit(code);
}

/// A GUI-subsystem exe has no console: use the one it was started from, unless output is
/// already redirected to a file or pipe.
fn attach_console() {
    use windows::Win32::System::Console::{AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE};
    // SAFETY: plain Win32 calls without pointers
    unsafe {
        let redirected = GetStdHandle(STD_OUTPUT_HANDLE).is_ok_and(|h| !h.is_invalid() && !h.0.is_null());
        if !redirected {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

fn print_json(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn gb(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1e9)
}

/// Settings from the app's database when it can be read, else the defaults.
fn disk_config() -> devdisk::Config {
    crate::db::Db::open()
        .ok()
        .map(|db| crate::commands::disk_config(&crate::engine::Settings::load(&db)))
        .unwrap_or_else(|| devdisk::Config {
            roots: devdisk::default_roots(),
            excludes: vec![],
            disabled: devdisk::default_disabled(),
            stale_days: devdisk::default_stale_days(),
        })
}

/// The Dev Disk scan, trimmed to what a person or an agent needs to decide.
fn disk_summary() -> Value {
    let r = devdisk::report(&disk_config());
    let safe: u64 = devdisk::auto_targets(&r).iter().map(|t| t.bytes).sum();
    json!({
        "drives": hardware::volumes().unwrap_or_default(),
        "safe_to_clean_bytes": safe,
        "caches": r.caches.iter().filter(|c| c.bytes > 0 || c.show_only).map(|c| json!({
            "id": c.id, "group": c.eco, "comes_back": c.tier, "bytes": if c.show_only { c.total } else { c.bytes },
            "only_shown": c.show_only, "to_recycle_bin": c.recycle, "in_use": c.busy, "never_clean": c.disabled, "paths": c.paths,
        })).collect::<Vec<_>>(),
        "projects": r.projects.iter().map(|p| json!({
            "path": p.path, "idle_days": p.idle_days, "stale": p.stale, "in_use": p.busy,
            "folders": p.items.iter().map(|i| json!({ "name": i.name, "kind": i.tier, "bytes": i.bytes })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "worktrees": r.worktrees.iter().map(|w| json!({ "path": w.path, "bytes": w.bytes, "idle_days": w.idle_days, "dirty": w.dirty, "orphan": w.orphan })).collect::<Vec<_>>(),
        "virtual_disks": r.vdisks,
        "note": "Projects, worktrees, the Recycle Bin and virtual disks are cleaned in the RigSweep app, where each step is confirmed. From the command line only caches that rebuild themselves can be cleaned (--disk-clean --safe).",
    })
}

fn print_report(v: &Value) {
    for d in v["drives"].as_array().into_iter().flatten() {
        println!("{}  {} free of {}", d["drive"].as_str().unwrap_or(""), gb(d["free"].as_u64().unwrap_or(0)), gb(d["total"].as_u64().unwrap_or(0)));
    }
    println!("\nSafe to clean now (caches that rebuild themselves): {}", gb(v["safe_to_clean_bytes"].as_u64().unwrap_or(0)));
    println!("\nCaches:");
    for c in v["caches"].as_array().into_iter().flatten() {
        println!("  {:<14} {:>10}  {}", c["id"].as_str().unwrap_or(""), gb(c["bytes"].as_u64().unwrap_or(0)), c["comes_back"].as_str().unwrap_or(""));
    }
    println!("\nProjects:");
    for p in v["projects"].as_array().into_iter().flatten() {
        let b: u64 = p["folders"].as_array().into_iter().flatten().map(|f| f["bytes"].as_u64().unwrap_or(0)).sum();
        println!("  {:>10}  {} (untouched {} days)", gb(b), p["path"].as_str().unwrap_or(""), p["idle_days"]);
    }
    for d in v["virtual_disks"].as_array().into_iter().flatten() {
        println!("\nVirtual disk {:>10}  {}  (compact it in the app)", gb(d["bytes"].as_u64().unwrap_or(0)), d["path"].as_str().unwrap_or(""));
    }
}

/// Empty the caches that rebuild themselves, waiting for the cleanup to finish.
fn clean_safe(dry_run: bool) -> Value {
    let cfg = disk_config();
    let targets = devdisk::auto_targets(&devdisk::report(&cfg));
    let planned: Vec<Value> = targets.iter().map(|t| json!({ "cache": t.id, "bytes": t.bytes })).collect();
    let total: u64 = targets.iter().map(|t| t.bytes).sum();
    if dry_run || targets.is_empty() {
        return json!({ "dry_run": dry_run, "would_free_bytes": total, "caches": planned });
    }
    let before = devdisk::system_free();
    let (tx, rx) = std::sync::mpsc::channel();
    if let Err(e) = devdisk::start(cfg, targets, |_| {}, move |r| {
        let _ = tx.send(r.clone());
    }) {
        return json!({ "error": e });
    }
    let Ok(r) = rx.recv() else { return json!({ "error": "the cleanup stopped unexpectedly" }) };
    if let Ok(db) = crate::db::Db::open() {
        for c in &r.cleaned {
            db.log("cli", None, "disk_clean", true, c);
        }
        db.add_impact("cli", &r.impact);
    }
    json!({ "freed_bytes": r.freed, "cleaned": r.cleaned, "skipped": r.skipped, "locked_files": r.locked_files,
        "system_drive_free_before": before, "system_drive_free_after": devdisk::system_free() })
}

fn print_clean(v: &Value) {
    if let Some(e) = v["error"].as_str() {
        println!("Error: {e}");
    } else if v["dry_run"].as_bool() == Some(true) {
        println!("Would free {}:", gb(v["would_free_bytes"].as_u64().unwrap_or(0)));
        for c in v["caches"].as_array().into_iter().flatten() {
            println!("  {:<14} {:>10}", c["cache"].as_str().unwrap_or(""), gb(c["bytes"].as_u64().unwrap_or(0)));
        }
    } else if v.get("freed_bytes").is_none() {
        println!("Nothing to clean.");
    } else {
        println!("Freed {}.", gb(v["freed_bytes"].as_u64().unwrap_or(0)));
        for s in v["skipped"].as_array().into_iter().flatten() {
            println!("  skipped: {}", s.as_str().unwrap_or(""));
        }
        if let Some(f) = v["system_drive_free_after"].as_u64() {
            println!("System drive now has {} free.", gb(f));
        }
    }
}

// ---------- MCP server (JSON-RPC 2.0, one message per line on stdin/stdout) ----------

fn tools() -> Value {
    json!([
        { "name": "drive_space", "description": "Size and free space of every drive on this Windows PC.",
          "inputSchema": { "type": "object", "properties": {} } },
        { "name": "disk_report", "description": "What takes disk space on this developer PC: package and tool caches (npm, pip, Cargo, Go, NuGet, Gradle, Playwright, Hugging Face, AI editors...), Windows leftovers, old build and dependency folders in projects, git worktrees left by AI agents, and Docker/WSL virtual disks. Read-only.",
          "inputSchema": { "type": "object", "properties": {} } },
        { "name": "free_disk_space", "description": "Empty only the caches that rebuild themselves on the next install or build (never projects, worktrees, the Recycle Bin or downloads that are slow to get back). Defaults to a dry run that only reports what would be freed; pass dry_run=false to clean.",
          "inputSchema": { "type": "object", "properties": { "dry_run": { "type": "boolean", "default": true } } } },
        { "name": "port_owner", "description": "Which processes hold a local TCP/UDP port (e.g. why port 3000 is already in use), with PID and command line.",
          "inputSchema": { "type": "object", "properties": { "port": { "type": "integer", "minimum": 1, "maximum": 65535 } }, "required": ["port"] } },
        { "name": "report_cleanup", "description": "After you deleted folders that RigSweep asked you to check (git worktrees, program leftovers), tell it which paths so it can count the space as freed with your help. Pass the exact paths only, never sizes: RigSweep checks each path is really gone and credits the size it measured itself. Paths it never handed to an AI, or that still exist, count for nothing. Deletes nothing.",
          "inputSchema": { "type": "object", "properties": { "paths": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 200 } }, "required": ["paths"] } },
    ])
}

fn call_tool(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "drive_space" => Ok(json!(hardware::volumes()?)),
        "disk_report" => Ok(disk_summary()),
        "free_disk_space" => Ok(clean_safe(args["dry_run"].as_bool().unwrap_or(true))),
        "report_cleanup" => {
            let paths: Vec<String> = args["paths"].as_array().ok_or("paths must be a list of folder paths")?.iter().filter_map(|p| p.as_str().map(str::to_string)).take(200).collect();
            if paths.is_empty() {
                return Err("paths must be a list of folder paths".into());
            }
            Ok(crate::handoff::report(&paths))
        }
        "port_owner" => {
            let p = args["port"].as_u64().filter(|p| (1..=65535).contains(p)).ok_or("port must be 1-65535")?;
            Ok(json!(netmon::port_owner(p as u16)))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}

fn mcp() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            let _ = writeln!(out, "{}", json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "parse error" } }));
            let _ = out.flush();
            continue;
        };
        let Some(id) = msg.get("id").cloned() else { continue }; // a notification: no reply
        let method = msg["method"].as_str().unwrap_or("");
        let reply = match method {
            "initialize" => Ok(json!({
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "rigsweep", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "RigSweep frees disk space on Windows developer PCs. Call disk_report first; free_disk_space only empties caches that rebuild themselves and is a dry run unless dry_run=false.",
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => {
                let r = call_tool(msg["params"]["name"].as_str().unwrap_or(""), &msg["params"]["arguments"]);
                Ok(match r {
                    Ok(v) => json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }], "isError": false }),
                    Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
                })
            }
            _ => Err(json!({ "code": -32601, "message": format!("method not found: {method}") })),
        };
        let resp = match reply {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        };
        let _ = writeln!(out, "{resp}");
        let _ = out.flush();
    }
}
