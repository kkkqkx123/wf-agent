//! CLI failure paths that exit before runtime bootstrap. These tests keep
//! startup validation honest without binding a port or loading a runtime.

use std::process::Command;

fn run(args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_wf-server"))
        .args(args)
        .output()
        .expect("server binary should run");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status.success(), stderr)
}

#[test]
fn invalid_log_level_fails_before_startup() {
    let (ok, stderr) = run(&["--log-level", "verbose"]);
    assert!(!ok, "unknown log level must fail");
    assert!(!stderr.trim().is_empty(), "clap should explain the failure");
}

#[test]
fn invalid_storage_spec_fails_before_startup() {
    let (ok, stderr) = run(&["--storage", "not-a-storage-spec"]);
    assert!(!ok, "invalid storage spec must fail");
    assert!(
        !stderr.trim().is_empty(),
        "startup should explain the failure"
    );
}

#[test]
fn unknown_flag_fails_before_startup() {
    let (ok, stderr) = run(&["--not-a-real-flag"]);
    assert!(!ok, "unknown flag must fail");
    assert!(!stderr.trim().is_empty(), "clap should explain the failure");
}
