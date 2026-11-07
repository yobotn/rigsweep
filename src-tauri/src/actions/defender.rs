use anyhow::Result;
use serde_json::{json, Value};

use super::{Ctx, Status};
use crate::util::{self, ps_quote};

#[derive(Clone, Debug, Default)]
pub struct MpPrefs {
    pub paths: Vec<String>,
    pub processes: Vec<String>,
}

pub fn read_prefs() -> Result<MpPrefs> {
    let out = util::powershell(
        "$p = Get-MpPreference; \
         'P:' + (($p.ExclusionPath | Where-Object { $_ }) -join '|'); \
         'X:' + (($p.ExclusionProcess | Where-Object { $_ }) -join '|')",
    )?;
    let mut prefs = MpPrefs::default();
    for line in out.lines() {
        let split = |s: &str| -> Vec<String> {
            s.split('|').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
        };
        if let Some(r) = line.strip_prefix("P:") {
            prefs.paths = split(r);
        } else if let Some(r) = line.strip_prefix("X:") {
            prefs.processes = split(r);
        }
    }
    Ok(prefs)
}

fn contains(list: &[String], item: &str) -> bool {
    let norm = |s: &str| s.trim_end_matches('\\').to_ascii_lowercase();
    list.iter().any(|x| norm(x) == norm(item))
}

fn wanted(ctx: &Ctx, processes: &[String], use_dev_folders: bool) -> (Vec<String>, Vec<String>) {
    let paths = if use_dev_folders {
        ctx.dev_folders.iter().map(|p| util::expand_env(p)).collect()
    } else {
        vec![]
    };
    (paths, processes.to_vec())
}

pub fn status(ctx: &Ctx, processes: &[String], use_dev_folders: bool) -> Result<Status> {
    let (paths, procs) = wanted(ctx, processes, use_dev_folders);
    if paths.is_empty() && procs.is_empty() {
        return Ok(Status::NotPresent);
    }
    let p = ctx.mp_prefs()?;
    let ok = paths.iter().all(|x| contains(&p.paths, x)) && procs.iter().all(|x| contains(&p.processes, x));
    Ok(if ok { Status::Applied } else { Status::NotApplied })
}

/// Original = which of our wanted items were already excluded before we touched anything.
pub fn read_current(ctx: &Ctx, processes: &[String], use_dev_folders: bool) -> Result<Value> {
    let (paths, procs) = wanted(ctx, processes, use_dev_folders);
    let p = ctx.mp_prefs()?;
    let pre_paths: Vec<_> = paths.iter().filter(|x| contains(&p.paths, x)).collect();
    let pre_procs: Vec<_> = procs.iter().filter(|x| contains(&p.processes, x)).collect();
    Ok(json!({ "preexisting_paths": pre_paths, "preexisting_processes": pre_procs }))
}

fn ps_list(items: &[String]) -> String {
    items.iter().map(|s| ps_quote(s)).collect::<Vec<_>>().join(",")
}

pub fn apply(ctx: &Ctx, processes: &[String], use_dev_folders: bool) -> Result<()> {
    let (paths, procs) = wanted(ctx, processes, use_dev_folders);
    let p = ctx.mp_prefs()?;
    let add_paths: Vec<String> = paths.into_iter().filter(|x| !contains(&p.paths, x)).collect();
    let add_procs: Vec<String> = procs.into_iter().filter(|x| !contains(&p.processes, x)).collect();
    if !add_paths.is_empty() {
        util::powershell(&format!("Add-MpPreference -ExclusionPath {}", ps_list(&add_paths)))?;
    }
    if !add_procs.is_empty() {
        util::powershell(&format!("Add-MpPreference -ExclusionProcess {}", ps_list(&add_procs)))?;
    }
    Ok(())
}

/// Remove exclusions we added (anything that wasn't there before).
pub fn revert(ctx: &Ctx, processes: &[String], use_dev_folders: bool, original: &Value) -> Result<()> {
    let (paths, procs) = wanted(ctx, processes, use_dev_folders);
    let pre = |k: &str| -> Vec<String> {
        original[k].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
    };
    let (pre_paths, pre_procs) = (pre("preexisting_paths"), pre("preexisting_processes"));
    let rm_paths: Vec<String> = paths.into_iter().filter(|x| !contains(&pre_paths, x)).collect();
    let rm_procs: Vec<String> = procs.into_iter().filter(|x| !contains(&pre_procs, x)).collect();
    remove(&rm_paths, &rm_procs)
}

pub fn remove(paths: &[String], procs: &[String]) -> Result<()> {
    if !paths.is_empty() {
        util::powershell(&format!("Remove-MpPreference -ExclusionPath {}", ps_list(paths)))?;
    }
    if !procs.is_empty() {
        util::powershell(&format!("Remove-MpPreference -ExclusionProcess {}", ps_list(procs)))?;
    }
    Ok(())
}
