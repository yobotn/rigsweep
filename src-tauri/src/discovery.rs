//! Vendor software found on this PC, for the anonymous report's raw `seen` section: services, startup
//! entries, scheduled tasks and running programs whose name (or the vendor folder they live in under
//! Program Files / ProgramData) matches a keyword in server/discovery.json, the list both sides share.
//!
//! What a row holds: the item's own name, its display name, its file name, the vendor folder name,
//! the keyword that matched, whether it runs or starts with Windows, its CPU and memory, and whether
//! the user turned it off with RigSweep. Never a path, and nothing that matches no keyword.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::analyzer::ProcInfo;
use crate::startup::StartupItem;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Seen {
    /// service | task | run | folder | process
    pub kind: String,
    pub name: String,
    pub display: String,
    /// file name only ("ArmouryCrate.Service.exe"), never its folder
    pub exe: String,
    /// the vendor folder directly under Program Files / ProgramData ("ASUS"), or ""
    pub folder: String,
    pub keyword: String,
    pub running: bool,
    /// starts with Windows or at sign-in
    pub startup: bool,
    /// turned off with a RigSweep tweak
    pub disabled: bool,
    /// processes: summed over all instances
    pub cpu: f32,
    pub mem: u64,
}

#[derive(Deserialize)]
struct List {
    keywords: Vec<String>,
}

pub fn keywords() -> &'static [String] {
    static K: OnceLock<Vec<String>> = OnceLock::new();
    K.get_or_init(|| {
        let l: List = serde_json::from_str(include_str!("../../server/discovery.json")).expect("server/discovery.json");
        l.keywords.into_iter().map(|k| k.to_lowercase()).collect()
    })
}

/// Lower-cased words of `s`: split on anything that isn't a letter or digit, and on camelCase
/// ("HPAppHelperCap" -> hp, app, helper, cap).
pub fn words(s: &str) -> Vec<String> {
    let mut out = vec![];
    for part in s.split(|c: char| !c.is_alphanumeric()).filter(|p| !p.is_empty()) {
        let cs: Vec<char> = part.chars().collect();
        let mut cur = String::new();
        for i in 0..cs.len() {
            let c = cs[i];
            let boundary = i > 0
                && ((c.is_uppercase() && cs[i - 1].is_lowercase())
                    || (c.is_uppercase() && cs[i - 1].is_uppercase() && cs.get(i + 1).is_some_and(|n| n.is_lowercase()))
                    || (c.is_ascii_digit() != cs[i - 1].is_ascii_digit()));
            if boundary && !cur.is_empty() {
                out.push(std::mem::take(&mut cur).to_lowercase());
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(cur.to_lowercase());
        }
    }
    out
}

/// The first keyword found in any of `texts`. Short keywords (4 letters or fewer) must be a whole
/// word ("hp" never matches "php"); longer ones may also start a word ("armoury" in "armourycrate").
pub fn keyword_in(texts: &[&str]) -> Option<String> {
    let ks = keywords();
    for t in texts {
        for w in words(t) {
            if let Some(k) = ks.iter().find(|k| w == **k || (k.len() > 4 && w.starts_with(k.as_str()))) {
                return Some(k.clone());
            }
        }
    }
    None
}

/// The folder right under Program Files, Program Files (x86) or ProgramData, e.g. "ASUS" for
/// C:\Program Files\ASUS\ArmouryCrate\x.exe. Anything elsewhere (user folders, projects) gives "".
pub fn vendor_folder(path: &str) -> String {
    let p = path.trim().trim_matches('"').replace('/', "\\");
    let roots: Vec<String> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramData"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .chain(["C:\\Program Files".into(), "C:\\Program Files (x86)".into(), "C:\\ProgramData".into()])
        .collect();
    for r in roots {
        let r = r.trim_end_matches('\\');
        if p.len() > r.len() + 1 && p[..r.len()].eq_ignore_ascii_case(r) && p.as_bytes()[r.len()] == b'\\' {
            return p[r.len() + 1..].split('\\').next().unwrap_or("").to_string();
        }
    }
    String::new()
}

pub fn file_name(path: &str) -> String {
    path.trim().trim_matches('"').rsplit(['\\', '/']).next().unwrap_or("").to_string()
}

/// Everything on this PC that matches a keyword. `enabled` = tweak ids in the user's profile.
pub fn collect(startup: &[StartupItem], procs: &[ProcInfo], enabled: &HashSet<String>) -> Vec<Seen> {
    let mut out: Vec<Seen> = vec![];
    for s in startup {
        let exe = file_name(&s.exe);
        let folder = vendor_folder(&s.exe);
        let Some(keyword) = keyword_in(&[&s.key, &s.name, &exe, &folder]) else { continue };
        out.push(Seen {
            kind: s.kind.to_string(),
            name: if s.kind == "service" || s.kind == "task" { s.key.rsplit('\\').next().unwrap_or(&s.key).to_string() } else { s.name.clone() },
            display: s.name.clone(),
            exe,
            folder,
            keyword,
            running: s.running,
            startup: true,
            disabled: s.tweak_id.as_ref().is_some_and(|t| enabled.contains(t)),
            cpu: 0.0,
            mem: 0,
        });
    }
    // running programs, one row per file name (a program with ten processes is one row)
    let mut by_exe: HashMap<String, Seen> = HashMap::new();
    for p in procs {
        let exe = if p.exe.is_empty() { p.name.clone() } else { file_name(&p.exe) };
        let folder = vendor_folder(&p.exe);
        let Some(keyword) = keyword_in(&[&p.name, &exe, &folder]) else { continue };
        let e = by_exe.entry(exe.to_lowercase()).or_insert_with(|| Seen {
            kind: "process".into(),
            name: p.name.clone(),
            display: String::new(),
            exe: exe.clone(),
            folder,
            keyword,
            running: true,
            startup: false,
            disabled: false,
            cpu: 0.0,
            mem: 0,
        });
        e.cpu += p.cpu;
        e.mem += p.mem;
    }
    let mut procs: Vec<Seen> = by_exe.into_values().map(|mut s| { s.cpu = (s.cpu * 10.0).round() / 10.0; s }).collect();
    procs.sort_by(|a, b| a.exe.cmp(&b.exe));
    out.extend(procs);
    // names are capped like the server expects; a report holds at most 300 rows
    for s in &mut out {
        for f in [&mut s.name, &mut s.display, &mut s.exe, &mut s.folder] {
            if f.chars().count() > 120 {
                *f = f.chars().take(120).collect();
            }
        }
    }
    out.truncate(300);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_split_on_case_and_symbols() {
        assert_eq!(words("HPAppHelperCap"), ["hp", "app", "helper", "cap"]);
        assert_eq!(words("ArmouryCrate.Service.exe"), ["armoury", "crate", "service", "exe"]);
        assert_eq!(words("NVDisplay.ContainerLocalSystem"), ["nv", "display", "container", "local", "system"]);
        assert_eq!(words("Logi Options+"), ["logi", "options"]);
    }

    #[test]
    fn keywords_match_vendors_not_lookalikes() {
        assert_eq!(keyword_in(&["ArmouryCrateService"]).as_deref(), Some("armoury"));
        assert_eq!(keyword_in(&["HPAppHelperCap"]).as_deref(), Some("hp"));
        assert_eq!(keyword_in(&["RtkAudUService64.exe", "Realtek"]).as_deref(), Some("realtek"));
        assert_eq!(keyword_in(&["NVIDIA Display Container LS"]).as_deref(), Some("nvidia"));
        assert_eq!(keyword_in(&["php-cgi.exe"]), None, "hp must be a whole word");
        assert_eq!(keyword_in(&["romantic.exe", "my-rogue-project"]), None, "rog must be a whole word");
        assert_eq!(keyword_in(&["node.exe", "Code.exe", "client-x-dashboard"]), None);
    }

    #[test]
    fn only_the_vendor_folder_of_program_files_is_kept() {
        assert_eq!(vendor_folder(r"C:\Program Files\ASUS\ArmouryCrate\Service.exe"), "ASUS");
        assert_eq!(vendor_folder(r#""C:\Program Files (x86)\Dell\SupportAssist\x.exe""#), "Dell");
        assert_eq!(vendor_folder(r"C:\ProgramData\Lenovo\Vantage\a.exe"), "Lenovo");
        assert_eq!(vendor_folder(r"C:\Users\me\code\asus-tools\run.exe"), "", "user folders never");
        assert_eq!(vendor_folder(r"D:\work\dell-client\app.exe"), "");
        assert_eq!(file_name(r"C:\Program Files\ASUS\x\ArmouryCrate.Service.exe"), "ArmouryCrate.Service.exe");
    }

    #[test]
    fn collect_keeps_vendor_items_and_merges_processes() {
        let p = |name: &str, exe: &str, cpu: f32, mem: u64| ProcInfo {
            pid: 1, start_time: 0, name: name.into(), exe: exe.into(), cpu, mem, disk_read: 0, disk_write: 0, services: vec![], protected: false,
        };
        let procs = vec![
            p("ArmouryCrate.UserSessionHelper.exe", r"C:\Program Files\ASUS\ArmouryCrate\ArmouryCrate.UserSessionHelper.exe", 1.5, 100),
            p("ArmouryCrate.UserSessionHelper.exe", r"C:\Program Files\ASUS\ArmouryCrate\ArmouryCrate.UserSessionHelper.exe", 0.5, 50),
            p("node.exe", r"C:\Users\me\code\secret-project\node.exe", 40.0, 900),
            p("helper.exe", r"C:\Program Files\Logitech\LogiOptionsPlus\helper.exe", 0.2, 30),
        ];
        let s = collect(&[], &procs, &HashSet::new());
        assert_eq!(s.len(), 2);
        let ac = s.iter().find(|x| x.exe.starts_with("ArmouryCrate")).unwrap();
        assert_eq!((ac.cpu, ac.mem, ac.keyword.as_str(), ac.folder.as_str()), (2.0, 150, "armoury", "ASUS"));
        let logi = s.iter().find(|x| x.exe == "helper.exe").unwrap();
        assert_eq!(logi.keyword, "logitech"); // matched through its vendor folder only
        assert!(s.iter().all(|x| !x.name.contains("secret") && !x.exe.contains('\\')));
    }
}
