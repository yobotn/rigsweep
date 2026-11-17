//! The app's real report (written by `cargo test -- --ignored dump_report` in src-tauri, path in
//! RIGSWEEP_REPORT) must parse and validate here. Skipped when the variable isn't set.

#[test]
fn app_report_passes_validation() {
    let Ok(path) = std::env::var("RIGSWEEP_REPORT") else { return };
    let json = std::fs::read_to_string(path).unwrap();
    let r: rigsweep_stats::model::Report = serde_json::from_str(&json).expect("app report parses");
    r.validate().expect("app report validates");
    assert!(r.top_cpu.iter().all(|a| a.app != "secret.exe"));
}
