//! Integration tests for the `pkexec` shim execution path.
//!
//! These tests verify that `run0-pkexec-shim` correctly constructs the argument
//! list and environment, and successfully executes the target `run0` binary
//! with the expected payload.

use assert_cmd::prelude::*;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::tempdir;

/// Creates a minimal fake `run0` script that records its argv and environment
/// to the files named by `$RUN0_ARGV_FILE` / `$RUN0_ENV_FILE`, then exits
/// with status 42 so callers can distinguish a successful exec from a crash.
fn write_fake_run0(dir: &Path) -> PathBuf {
    let run0 = dir.join("run0");

    fs::write(
        &run0,
        r#"#!/bin/sh
printf '%s\n' "$@" > "$RUN0_ARGV_FILE"
env > "$RUN0_ENV_FILE"
exit 42
"#,
    )
    .unwrap();

    let mut perm = fs::metadata(&run0).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&run0, perm).unwrap();

    run0
}

#[test]
fn exec_path_forwards_argv_and_env() {
    let dir = tempdir().unwrap();
    let run0 = write_fake_run0(dir.path());
    let argv_file = dir.path().join("argv.txt");
    let env_file = dir.path().join("env.txt");

    Command::cargo_bin("run0-pkexec-shim")
        .unwrap()
        .env("RUN0_BIN", &run0)
        .env("RUN0_ARGV_FILE", &argv_file)
        .env("RUN0_ENV_FILE", &env_file)
        .args(["--keep-cwd", "--user", "root", "id"])
        .assert()
        .code(42);

    let argv = fs::read_to_string(&argv_file).unwrap();
    let lines: Vec<_> = argv.lines().collect();

    assert_eq!(lines[0], "--user");
    assert_eq!(lines[1], "root");

    let setenv_pos = lines
        .iter()
        .position(|x| *x == "--setenv")
        .expect("--setenv not found");

    assert!(lines[setenv_pos + 1].starts_with("PKEXEC_UID="), "{argv}");

    // Verify that the `--` separator is correctly injected before the target program
    let command_separator_pos = lines
        .iter()
        .position(|x| *x == "--")
        .expect("-- separator not found");

    assert_eq!(lines[command_separator_pos + 1], "id");
    assert_eq!(lines.last(), Some(&"id"));

    let env = fs::read_to_string(&env_file).unwrap();
    assert!(env.contains("RUN0_ARGV_FILE="));
    assert!(env.contains("RUN0_ENV_FILE="));
}

#[test]
fn exec_path_forwards_extra_args() {
    let dir = tempdir().unwrap();
    let run0 = write_fake_run0(dir.path());
    let argv_file = dir.path().join("argv_extra.txt");
    let env_file = dir.path().join("env_extra.txt");

    Command::cargo_bin("run0-pkexec-shim")
        .unwrap()
        .env("RUN0_BIN", &run0)
        .env("RUN0_ARGV_FILE", &argv_file)
        .env("RUN0_ENV_FILE", &env_file)
        .args([
            "--keep-cwd",
            "--run0-extra-arg",
            "--background",
            "--run0-extra-arg=--nice=10",
            "ls",
            "-la",
        ])
        .assert()
        .code(42);

    let argv = fs::read_to_string(&argv_file).unwrap();
    let lines: Vec<_> = argv.lines().collect();

    // Verify that extra arguments are seamlessly passed to run0
    assert!(lines.contains(&"--background"));
    assert!(lines.contains(&"--nice=10"));

    // Verify that the actual target command and its arguments safely follow the `--`
    let command_separator_pos = lines
        .iter()
        .position(|x| *x == "--")
        .expect("-- separator not found");

    assert_eq!(lines[command_separator_pos + 1], "ls");
    assert_eq!(lines[command_separator_pos + 2], "-la");
}

#[test]
fn exec_path_via_shell_when_no_command_given() {
    let dir = tempdir().unwrap();
    let run0 = write_fake_run0(dir.path());
    let argv_file = dir.path().join("argv_shell.txt");
    let env_file = dir.path().join("env_shell.txt");

    Command::cargo_bin("run0-pkexec-shim")
        .unwrap()
        .env("RUN0_BIN", &run0)
        .env("RUN0_ARGV_FILE", &argv_file)
        .env("RUN0_ENV_FILE", &env_file)
        .args(["--keep-cwd"])
        .assert()
        .code(42);

    let argv = fs::read_to_string(&argv_file).unwrap();
    let lines: Vec<_> = argv.lines().collect();

    assert!(
        lines.contains(&"--via-shell"),
        "--via-shell must be present when no command is given; got: {argv}"
    );
    assert!(
        !lines.contains(&"--"),
        "-- separator must not appear without a command; got: {argv}"
    );
}
