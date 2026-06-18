use crate::{
    cli::Opts,
    user::{current_uid, lookup_target_home},
};
use std::{
    env,
    ffi::{OsStr, OsString},
};

/// Returns the path to the `run0` binary.
///
/// Reads `$RUN0_BIN` from the environment, falling back to `"run0"` (i.e.
/// PATH lookup) if the variable is absent or empty.
#[must_use]
pub fn run0_bin() -> OsString {
    env::var_os("RUN0_BIN").unwrap_or_else(|| OsString::from("run0"))
}

/// Constructs the argument list to pass to `run0`.
///
/// The resulting vector does **not** include the binary name itself; callers
/// are expected to pass it via [`Command::new`] or equivalent.
///
/// Argument order: `--user`, optional `--chdir`, `--setenv PKEXEC_UID=…`,
/// any `--run0-extra-arg` values, then `--via-shell` or `-- <command…>`.
#[must_use]
pub fn build_run0_argv(opts: &Opts) -> Vec<OsString> {
    let target_user = opts
        .user
        .as_deref()
        .unwrap_or(OsStr::new("root"))
        .to_os_string();

    let mut v: Vec<OsString> = Vec::new();

    v.push("--user".into());
    v.push(target_user.clone());

    if !opts.keep_cwd {
        if let Some(home) = lookup_target_home(&target_user) {
            v.push("--chdir".into());
            v.push(home);
        }
    } else if let Ok(cwd) = env::current_dir() {
        v.push("--chdir".into());
        v.push(cwd.into_os_string());
    }

    v.push("--setenv".into());
    v.push(format!("PKEXEC_UID={}", current_uid()).into());

    v.extend(opts.run0_args.iter().cloned());

    if opts.command.is_empty() {
        v.push("--via-shell".into());
    } else {
        v.push("--".into());
        v.extend(opts.command.iter().cloned());
    }

    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strips the `--chdir <value>` pair from a `build_run0_argv` result so that
    /// tests that don't care about the working-directory argument remain stable
    /// across environments where the home directory may differ.
    fn argv_without_chdir(opts: &Opts) -> Vec<OsString> {
        let v = build_run0_argv(opts);
        let mut out = Vec::new();
        let mut skip_next = false;
        for arg in &v {
            if skip_next {
                skip_next = false;
                continue;
            }
            if arg == "--chdir" {
                skip_next = true;
                continue;
            }
            out.push(arg.clone());
        }
        out
    }

    #[test]
    fn run0_bin_default_is_run0() {
        temp_env::with_var_unset("RUN0_BIN", || {
            assert_eq!(run0_bin(), OsString::from("run0"));
        });
    }

    #[test]
    fn run0_bin_reads_env_override() {
        temp_env::with_var("RUN0_BIN", Some("/usr/local/bin/run0"), || {
            assert_eq!(run0_bin(), OsString::from("/usr/local/bin/run0"));
        });
    }

    #[test]
    fn argv_defaults_to_root_via_shell() {
        let opts = Opts {
            keep_cwd: false,
            user: None,
            run0_args: vec![],
            command: vec![],
        };

        let v = argv_without_chdir(&opts);
        assert_eq!(v[0], "--user");
        assert_eq!(v[1], "root");
        assert!(v.contains(&OsString::from("--via-shell")));
    }

    #[test]
    fn argv_explicit_user() {
        let opts = Opts {
            keep_cwd: false,
            user: Some(OsString::from("alice")),
            run0_args: vec![],
            command: vec![OsString::from("id")],
        };

        let v = argv_without_chdir(&opts);
        assert_eq!(v[0], "--user");
        assert_eq!(v[1], "alice");
        assert!(v.contains(&OsString::from("id")));
        assert!(!v.contains(&OsString::from("--via-shell")));
    }

    #[test]
    fn argv_keep_cwd_specifies_caller_cwd() {
        let opts = Opts {
            keep_cwd: true,
            user: Some(OsString::from("root")),
            run0_args: vec![],
            command: vec![OsString::from("id")],
        };
        let v = build_run0_argv(&opts);
        let pos = v.iter().position(|a| a == "--chdir").unwrap();
        let expected_cwd = env::current_dir().unwrap().into_os_string();
        assert_eq!(v[pos + 1], expected_cwd);
    }

    #[test]
    fn argv_always_sets_pkexec_uid() {
        let opts = Opts {
            keep_cwd: true,
            user: None,
            run0_args: vec![],
            command: vec![],
        };
        let v = build_run0_argv(&opts);
        let pos = v.iter().position(|a| a == "--setenv").unwrap();
        let val = &v[pos + 1];
        assert!(val.to_string_lossy().starts_with("PKEXEC_UID="));
    }

    #[test]
    fn argv_pkexec_uid_matches_current_uid() {
        let opts = Opts {
            keep_cwd: true,
            user: None,
            run0_args: vec![],
            command: vec![],
        };
        let v = build_run0_argv(&opts);
        let pos = v.iter().position(|a| a == "--setenv").unwrap();
        let got = v[pos + 1].to_str().unwrap();
        let expected = format!("PKEXEC_UID={}", current_uid());
        assert_eq!(got, expected);
    }

    #[test]
    fn argv_includes_run0_extra_args() {
        let opts = Opts {
            keep_cwd: true,
            user: None,
            run0_args: vec![OsString::from("--background"), OsString::from("--nice=10")],
            command: vec![OsString::from("id")],
        };
        let v = build_run0_argv(&opts);
        assert!(v.contains(&OsString::from("--background")));
        assert!(v.contains(&OsString::from("--nice=10")));
    }

    #[test]
    fn argv_numeric_uid_user_forwarded_verbatim() {
        let opts = Opts {
            keep_cwd: true,
            user: Some(OsString::from("1000")),
            run0_args: vec![],
            command: vec![OsString::from("id")],
        };
        let v = build_run0_argv(&opts);
        assert_eq!(v[0], "--user");
        assert_eq!(v[1], "1000");
    }

    #[test]
    fn argv_hash_uid_user_forwarded_verbatim() {
        let opts = Opts {
            keep_cwd: true,
            user: Some(OsString::from("#0")),
            run0_args: vec![],
            command: vec![OsString::from("whoami")],
        };
        let v = build_run0_argv(&opts);
        assert_eq!(v[1], "#0");
    }

    #[test]
    fn argv_no_via_shell_when_command_present() {
        let opts = Opts {
            keep_cwd: true,
            user: None,
            run0_args: vec![],
            command: vec![OsString::from("ls")],
        };
        let v = build_run0_argv(&opts);
        assert!(!v.contains(&OsString::from("--via-shell")));
    }

    #[test]
    fn argv_stable_argument_order() {
        let opts = Opts {
            keep_cwd: true,
            user: Some(OsString::from("root")),
            run0_args: vec![],
            command: vec![OsString::from("id")],
        };
        let v = build_run0_argv(&opts);
        let user_pos = v.iter().position(|a| a == "--user").unwrap();
        let setenv_pos = v.iter().position(|a| a == "--setenv").unwrap();
        let cmd_pos = v.iter().position(|a| a == "id").unwrap();
        assert!(user_pos < setenv_pos);
        assert!(setenv_pos < cmd_pos);
    }

    #[test]
    fn argv_command_forwarded_verbatim() {
        let opts = Opts {
            keep_cwd: true,
            user: None,
            run0_args: vec![],
            command: vec![
                OsString::from("bash"),
                OsString::from("-c"),
                OsString::from("echo hello"),
            ],
        };
        let v = build_run0_argv(&opts);
        let pos = v.iter().position(|a| a == "bash").unwrap();
        assert_eq!(v[pos + 1], "-c");
        assert_eq!(v[pos + 2], "echo hello");
    }

    #[test]
    fn argv_no_chdir_when_home_lookup_fails() {
        // UID 4294967294 (u32::MAX − 1) is well outside the range allocated by
        // any real system, so the passwd lookup is guaranteed to return None.
        let opts = Opts {
            keep_cwd: false,
            user: Some(OsString::from("4294967294")),
            run0_args: vec![],
            command: vec![],
        };
        let v = build_run0_argv(&opts);
        assert!(!v.contains(&OsString::from("--chdir")));
    }

    #[test]
    fn argv_chdir_targets_home_when_keep_cwd_false() {
        // When keep_cwd is false (the default), --chdir must point to the target
        // user's home directory so run0 lands in the right place.
        let opts = Opts {
            keep_cwd: false,
            user: Some(OsString::from("root")),
            run0_args: vec![],
            command: vec![OsString::from("id")],
        };
        let v = build_run0_argv(&opts);
        let pos = v
            .iter()
            .position(|a| a == "--chdir")
            .expect("--chdir must be present for a known user");
        let expected = lookup_target_home(&OsString::from("root")).unwrap();
        assert_eq!(v[pos + 1], expected);
    }
}
