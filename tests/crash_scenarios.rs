#![cfg(feature = "std")]

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("flashdb-crash-scenario-{name}-{nanos}.bin"))
}

fn run_harness(args: &[&str]) {
    let harness = std::env::var("CARGO_BIN_EXE_flashdb-crash-harness")
        .expect("cargo should provide crash harness binary path");
    let status = Command::new(harness)
        .args(args)
        .status()
        .expect("crash harness should launch");
    assert!(status.success(), "harness failed for args: {args:?}");
}

#[test]
fn kv_process_restart_recovers_from_pre_write_tail() {
    let path = unique_path("kv-pre-write-tail");
    let path_str = path.to_str().unwrap();

    run_harness(&["kv-init-stable", path_str]);
    run_harness(&["kv-inject-prewrite-tail", path_str]);
    run_harness(&["kv-check-stable-and-write-fresh", path_str]);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn kv_process_restart_recovers_from_crc_mismatched_tail() {
    let path = unique_path("kv-crc-tail");
    let path_str = path.to_str().unwrap();

    run_harness(&["kv-init-answer", path_str]);
    run_harness(&["kv-inject-crc-tail", path_str]);
    run_harness(&["kv-check-answer-and-write-fresh", path_str]);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn tsdb_process_restart_recovers_from_pre_write_tail() {
    let path = unique_path("ts-pre-write-tail");
    let path_str = path.to_str().unwrap();

    run_harness(&["ts-init-seed", path_str]);
    run_harness(&["ts-inject-prewrite-tail", path_str]);
    run_harness(&["ts-check-seed-and-append-fresh", path_str]);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn tsdb_process_restart_preserves_query_and_reverse_iteration_after_reopen() {
    let path = unique_path("ts-reboot-query");
    let path_str = path.to_str().unwrap();

    run_harness(&["ts-init-window", path_str]);
    run_harness(&["ts-check-window-query", path_str]);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn tsdb_process_restart_preserves_status_mutation_across_reopen() {
    let path = unique_path("ts-status-reboot");
    let path_str = path.to_str().unwrap();

    run_harness(&["ts-init-status-window", path_str]);
    run_harness(&["ts-set-status-and-reboot-check", path_str]);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn tsdb_process_restart_preserves_clean_reset_across_reopen() {
    let path = unique_path("ts-clean-reboot");
    let path_str = path.to_str().unwrap();

    run_harness(&["ts-init-clean-window", path_str]);
    run_harness(&["ts-clean-and-reboot-check", path_str]);

    std::fs::remove_file(path).unwrap();
}
