use assert_cmd::prelude::*;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::tempdir;

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

    assert_eq!(lines.last(), Some(&"id"));

    let env = fs::read_to_string(&env_file).unwrap();
    assert!(env.contains("RUN0_ARGV_FILE="));
    assert!(env.contains("RUN0_ENV_FILE="));
}
