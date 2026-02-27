use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Returns a Command pointing to the compiled binary, running from /tmp
/// (to avoid dotenvy loading the project's .env) and with DATABASE_URL removed.
fn cli() -> Command {
    let bin = cargo_bin_path();
    let mut cmd = Command::new(bin);
    // Run from /tmp so dotenvy doesn't load the project's .env file
    cmd.current_dir(std::env::temp_dir());
    cmd.env_remove("DATABASE_URL");
    cmd
}

/// Finds the compiled test binary path.
fn cargo_bin_path() -> PathBuf {
    // During `cargo test`, the binary is compiled into the same target dir.
    let mut path = std::env::current_exe()
        .unwrap()
        .parent() // deps/
        .unwrap()
        .parent() // debug/
        .unwrap()
        .to_path_buf();
    path.push("ruc_finder");
    path
}

// ---------------------------------------------------------------------------
// --help
// ---------------------------------------------------------------------------

#[test]
fn cli_help_shows_usage() {
    let output = cli().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("RUC Finder"));
    assert!(stdout.contains("--sync"));
    assert!(stdout.contains("--force"));
    assert!(stdout.contains("--format"));
    assert!(stdout.contains("--backfill-hashes"));
}

// ---------------------------------------------------------------------------
// --force is a valid clap flag (checked via --help output)
// ---------------------------------------------------------------------------

#[test]
fn cli_force_flag_recognized_in_help() {
    let output = cli().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--force"));
    assert!(stdout.contains("bypass"));
}

// ---------------------------------------------------------------------------
// --sync --format with invalid format
// ---------------------------------------------------------------------------

#[test]
fn cli_sync_invalid_format_exits_with_error() {
    let output = cli()
        .args(["--sync", "--format", "xml"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unknown format"));
}

// ---------------------------------------------------------------------------
// --sync --format json --force succeeds as valid combination (no DB needed)
// This starts sync-to-file which tries to fetch the DNIT page — it may fail
// due to network, but clap should accept the flags without complaint.
// ---------------------------------------------------------------------------

#[test]
fn cli_sync_force_format_accepted_by_clap() {
    use std::process::Stdio;
    let mut child = cli()
        .args(["--sync", "--force", "--format", "json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Give it a moment to pass clap validation, then kill.
    std::thread::sleep(Duration::from_secs(2));
    let _ = child.kill();
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT contain clap error
    assert!(
        !stderr.contains("unexpected argument"),
        "clap rejected --force with --sync --format"
    );
}

// ---------------------------------------------------------------------------
// --backfill-hashes without DATABASE_URL fails gracefully
// ---------------------------------------------------------------------------

#[test]
fn cli_backfill_hashes_without_db_fails() {
    let output = cli()
        .arg("--backfill-hashes")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("DATABASE_URL"));
}

// ---------------------------------------------------------------------------
// No DATABASE_URL, no --sync → server can't start
// ---------------------------------------------------------------------------

#[test]
fn cli_no_db_no_sync_fails() {
    let output = cli().output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("DATABASE_URL"));
}

// ---------------------------------------------------------------------------
// Unknown flag rejected by clap
// ---------------------------------------------------------------------------

#[test]
fn cli_unknown_flag_rejected() {
    let output = cli().arg("--nonexistent-flag").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument") || stderr.contains("error"));
}
