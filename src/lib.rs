//! # run0-pkexec-shim
//!
//! A compatibility shim that implements the `pkexec(1)` command-line interface
//! but delegates execution to `run0(1)` (systemd's unprivileged privilege
//! escalation tool) instead of polkit.
//!
//! ## Argument mapping
//!
//! | pkexec flag            | run0 equivalent                          |
//! |------------------------|------------------------------------------|
//! | `--user USER`          | `--user USER`                            |
//! | `--keep-cwd` (absent)  | `--chdir <target home>`                  |
//! | `--keep-cwd` (present) | *(omitted — run0 keeps the caller's cwd)*|
//! | *(always)*             | `--setenv PKEXEC_UID=<caller uid>`       |
//! | *(no PROGRAM)*         | `--via-shell`                            |
//!
//! ## Environment
//!
//! Set `RUN0_BIN` to override the path to the `run0` binary (default: `"run0"`).

use std::{
    env,
    ffi::{OsStr, OsString},
    process::exit,
};
use uzers::os::unix::UserExt;

/// The crate version, taken from `Cargo.toml` at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// ────────────────────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────────────────────

/// Parsed representation of the pkexec command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opts {
    /// When `true` (`--keep-cwd`), do **not** switch to the target user's home
    /// directory; instead keep the caller's current working directory.
    pub keep_cwd: bool,

    /// Target user name, numeric UID, or `#UID` form.
    /// Defaults to `"root"` if absent.
    pub user: Option<OsString>,

    /// The program and its arguments to execute.
    /// If empty, `run0` is invoked with `--via-shell`.
    pub command: Vec<OsString>,
}

// ────────────────────────────────────────────────────────────────────────────
// Environment helpers
// ────────────────────────────────────────────────────────────────────────────

/// Returns the path (or name) of the `run0` binary to invoke.
///
/// Reads `$RUN0_BIN`; falls back to `"run0"` (i.e. PATH lookup).
#[must_use]
pub fn run0_bin() -> OsString {
    env::var_os("RUN0_BIN").unwrap_or_else(|| OsString::from("run0"))
}

/// Returns the real UID of the calling process.
#[must_use]
pub fn current_uid() -> u32 {
    uzers::get_current_uid()
}

// ────────────────────────────────────────────────────────────────────────────
// Help / version
// ────────────────────────────────────────────────────────────────────────────

/// Prints the help message to stdout and returns.
///
/// The caller is expected to `exit(0)` immediately after (as `parse_args` does).
pub fn print_help() {
    println!(
        r#"Shim for the pkexec command that utilizes run0

Usage:
    pkexec [OPTIONS] [PROGRAM] [ARGUMENTS...]

Options:
    --user USER
        run command as specified user name or ID

    --keep-cwd
        keep the current working directory instead of switching to
        the target user's home directory

    --disable-internal-agent
        ignored (compatibility with pkexec)

    --help
        show this help message

    --version
        show version information
"#
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Argument parsing
// ────────────────────────────────────────────────────────────────────────────

/// Parses `std::env::args_os()` into [`Opts`].
///
/// Exits the process for `--help` and `--version`.
pub fn parse_args() -> Opts {
    parse_args_from(env::args_os())
}

/// Parses an arbitrary iterator of `OsString` arguments into [`Opts`].
///
/// The first item is treated as `argv[0]` (the program name) and is skipped.
///
/// # Exits
///
/// - `0` for `--help` and `--version`
/// - `2` if `--user` is given without a following value
pub fn parse_args_from<I>(argv: I) -> Opts
where
    I: IntoIterator<Item = OsString>,
{
    let mut argv = argv.into_iter();
    let _ = argv.next();

    let mut keep_cwd = false;
    let mut user = None;
    let mut command = Vec::new();

    while let Some(arg) = argv.next() {
        match arg.to_str() {
            Some("--version") => {
                println!("run0-pkexec-shim {VERSION}");
                exit(0);
            }
            Some("--help") => {
                print_help();
                exit(0);
            }
            Some("--") => {
                command.extend(argv);
                break;
            }
            Some("--disable-internal-agent") => {}
            Some("--keep-cwd") => keep_cwd = true,
            Some("--user") => {
                let v = argv.next().unwrap_or_else(|| {
                    eprintln!("pkexec: missing --user argument");
                    exit(2);
                });
                user = Some(v);
            }
            Some(s) if s.starts_with("--user=") => {
                user = Some(OsString::from(s.strip_prefix("--user=").unwrap()));
            }
            _ => {
                command.push(arg);
                command.extend(argv);
                break;
            }
        }
    }

    Opts {
        keep_cwd,
        user,
        command,
    }
}

// ────────────────────────────────────────────────────────────────────────────
// User / UID helpers
// ────────────────────────────────────────────────────────────────────────────

/// Parses a user specifier into a numeric UID, if possible.
///
/// Accepts plain decimals (`"0"`, `"1000"`) and the `#UID` prefix form
/// (`"#0"`, `"#1000"`).  Returns `None` for non-numeric strings such as
/// `"root"` or `"alice"`.
#[must_use]
pub fn parse_uid_spec(user: &OsString) -> Option<u32> {
    let s = user.to_str()?;

    let s = match s.strip_prefix('#') {
        Some("") => return None,
        Some(rest) => rest,
        None => s,
    };

    s.parse().ok()
}

/// Returns `true` if `user` refers to the root account (UID 0 or the name
/// `"root"`).
#[must_use]
pub fn user_is_root(user: &OsString) -> bool {
    match parse_uid_spec(user) {
        Some(uid) => uid == 0,
        None => user.as_os_str() == OsStr::new("root"),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ── User lookups
// ────────────────────────────────────────────────────────────────────────────

/// Looks up the home directory for the given user **name**.
///
/// Returns `None` if the user does not exist, the name is not valid UTF-8,
/// or the home directory path is empty.
#[must_use]
pub fn lookup_home_by_name(name: &OsStr) -> Option<OsString> {
    let home = uzers::get_user_by_name(name.to_str()?)?
        .home_dir()
        .as_os_str()
        .to_os_string();
    if home.is_empty() { None } else { Some(home) }
}

/// Looks up the home directory for the given **UID**.
///
/// Returns `None` if the UID does not exist or the home directory path is empty.
#[must_use]
pub fn lookup_home_by_uid(uid: u32) -> Option<OsString> {
    let home = uzers::get_user_by_uid(uid)?
        .home_dir()
        .as_os_str()
        .to_os_string();
    if home.is_empty() { None } else { Some(home) }
}

/// Resolves the home directory for `user`, which may be a name, a plain UID,
/// or a `#UID`-prefixed string.
#[must_use]
pub fn lookup_target_home(user: &OsString) -> Option<OsString> {
    if let Some(uid) = parse_uid_spec(user) {
        return lookup_home_by_uid(uid);
    }
    lookup_home_by_name(user.as_os_str())
}

// ────────────────────────────────────────────────────────────────────────────
// run0 argv construction
// ────────────────────────────────────────────────────────────────────────────

/// Builds the argument list to pass to `run0`.
///
/// # Behaviour
///
/// * Always passes `--user <target>` (defaults to `root`).
/// * Unless `--keep-cwd` was given, passes `--chdir <target_home>` when the
///   home directory can be resolved.  This matches the real pkexec behaviour
///   of running the program in the target user's home directory by default.
///   Omitting `--chdir` when `keep_cwd` is true causes `run0` to inherit the
///   caller's working directory naturally.
/// * Always passes `--setenv PKEXEC_UID=<caller_uid>` for compatibility with
///   programs that read this variable.
/// * Passes `--via-shell` when no PROGRAM was given; otherwise appends the
///   PROGRAM and its ARGUMENTS verbatim.
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

    // pkexec always changes to the target user's home unless --keep-cwd is set.
    // When keep_cwd is true we simply omit --chdir; run0 then keeps the
    // caller's cwd without any extra work on our part.
    if !opts.keep_cwd
        && let Some(home) = lookup_target_home(&target_user)
    {
        v.push("--chdir".into());
        v.push(home);
    }

    v.push("--setenv".into());
    v.push(format!("PKEXEC_UID={}", current_uid()).into());

    if opts.command.is_empty() {
        v.push("--via-shell".into());
    } else {
        v.extend(opts.command.iter().cloned());
    }

    v
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn args(argv: &[&str]) -> impl Iterator<Item = OsString> {
        let mut v: Vec<OsString> = vec![OsString::from("pkexec")];
        v.extend(argv.iter().map(OsString::from));
        v.into_iter()
    }

    // ── run0_bin ──────────────────────────────────────────────────────────────

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

    // ── parse_args_from ───────────────────────────────────────────────────────

    #[test]
    fn parse_empty_args() {
        let opts = parse_args_from(args(&[]));
        assert!(!opts.keep_cwd);
        assert!(opts.user.is_none());
        assert!(opts.command.is_empty());
    }

    #[test]
    fn parse_keep_cwd() {
        let opts = parse_args_from(args(&["--keep-cwd"]));
        assert!(opts.keep_cwd);
    }

    #[test]
    fn parse_user_space_separated() {
        let opts = parse_args_from(args(&["--user", "alice"]));
        assert_eq!(opts.user.as_deref(), Some(OsStr::new("alice")));
    }

    #[test]
    fn parse_user_equals_form() {
        let opts = parse_args_from(args(&["--user=bob"]));
        assert_eq!(opts.user.as_deref(), Some(OsStr::new("bob")));
    }

    /// Only the first '=' is the separator; '=' characters in the value must
    /// survive verbatim.
    #[test]
    fn parse_user_equals_form_with_embedded_equals() {
        let opts = parse_args_from(args(&["--user=a=b"]));
        assert_eq!(opts.user.as_deref(), Some(OsStr::new("a=b")));
    }

    /// When --user appears more than once the last occurrence wins (matches
    /// typical POSIX option-parsing convention).
    #[test]
    fn parse_user_last_occurrence_wins() {
        let opts = parse_args_from(args(&["--user", "alice", "--user", "bob"]));
        assert_eq!(opts.user.as_deref(), Some(OsStr::new("bob")));
    }

    #[test]
    fn parse_command_after_options() {
        let opts = parse_args_from(args(&["--user", "alice", "ls", "-la", "/tmp"]));
        assert_eq!(
            opts.command,
            vec![
                OsString::from("ls"),
                OsString::from("-la"),
                OsString::from("/tmp"),
            ]
        );
    }

    #[test]
    fn parse_command_stops_at_first_non_flag() {
        let opts = parse_args_from(args(&["ls", "--keep-cwd"]));
        assert!(!opts.keep_cwd);
        assert_eq!(
            opts.command,
            vec![OsString::from("ls"), OsString::from("--keep-cwd")]
        );
    }

    #[test]
    fn parse_disable_internal_agent_is_ignored() {
        let opts = parse_args_from(args(&["--disable-internal-agent", "id"]));
        assert_eq!(opts.command, vec![OsString::from("id")]);
    }

    #[test]
    fn parse_all_flags_combined() {
        let opts = parse_args_from(args(&["--keep-cwd", "--user=carol", "env"]));
        assert!(opts.keep_cwd);
        assert_eq!(opts.user.as_deref(), Some(OsStr::new("carol")));
        assert_eq!(opts.command, vec![OsString::from("env")]);
    }

    // ── parse_uid_spec ────────────────────────────────────────────────────────

    #[test]
    fn uid_spec_plain_zero() {
        assert_eq!(parse_uid_spec(&OsString::from("0")), Some(0));
    }

    #[test]
    fn uid_spec_plain_number() {
        assert_eq!(parse_uid_spec(&OsString::from("1000")), Some(1000));
    }

    #[test]
    fn uid_spec_hash_prefix() {
        assert_eq!(parse_uid_spec(&OsString::from("#1000")), Some(1000));
    }

    #[test]
    fn uid_spec_hash_zero() {
        assert_eq!(parse_uid_spec(&OsString::from("#0")), Some(0));
    }

    #[test]
    fn uid_spec_name_returns_none() {
        assert_eq!(parse_uid_spec(&OsString::from("root")), None);
        assert_eq!(parse_uid_spec(&OsString::from("alice")), None);
    }

    #[test]
    fn uid_spec_negative_returns_none() {
        assert_eq!(parse_uid_spec(&OsString::from("-1")), None);
    }

    /// Values that exceed u32::MAX must not wrap around silently.
    #[test]
    fn uid_spec_u32_overflow_returns_none() {
        assert_eq!(parse_uid_spec(&OsString::from("4294967296")), None);
    }

    /// Non-UTF-8 byte sequences cannot be parsed as a number.
    #[test]
    fn uid_spec_non_utf8_returns_none() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![0xFF, 0xFE]);
        assert_eq!(parse_uid_spec(&bad), None);
    }

    // ── user_is_root ──────────────────────────────────────────────────────────

    #[test]
    fn root_by_name() {
        assert!(user_is_root(&OsString::from("root")));
    }

    #[test]
    fn root_by_uid_zero() {
        assert!(user_is_root(&OsString::from("0")));
    }

    #[test]
    fn root_by_hash_uid_zero() {
        assert!(user_is_root(&OsString::from("#0")));
    }

    #[test]
    fn non_root_name() {
        assert!(!user_is_root(&OsString::from("alice")));
    }

    #[test]
    fn non_root_uid() {
        assert!(!user_is_root(&OsString::from("1000")));
    }

    // ── build_run0_argv ───────────────────────────────────────────────────────

    /// Strip `--chdir <VALUE>` pairs from a run0 argv; the chdir value depends
    /// on the host /etc/passwd, making it unsuitable for exact-match assertions.
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
    fn argv_defaults_to_root_via_shell() {
        let opts = Opts {
            keep_cwd: false,
            user: None,
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
            command: vec![OsString::from("id")],
        };
        let v = argv_without_chdir(&opts);
        assert_eq!(v[0], "--user");
        assert_eq!(v[1], "alice");
        assert!(v.contains(&OsString::from("id")));
        assert!(!v.contains(&OsString::from("--via-shell")));
    }

    #[test]
    fn argv_keep_cwd_suppresses_chdir() {
        let opts = Opts {
            keep_cwd: true,
            user: Some(OsString::from("root")),
            command: vec![OsString::from("id")],
        };
        let v = build_run0_argv(&opts);
        assert!(
            !v.contains(&OsString::from("--chdir")),
            "expected no --chdir when keep_cwd is true, got: {v:?}"
        );
    }

    /// --chdir must be absent when home lookup fails and keep_cwd is false.
    /// This exercises the silent-fallback behaviour (no panic, no --chdir "").
    #[test]
    fn argv_no_chdir_when_home_lookup_fails() {
        // UID 4294967294 virtually never exists on any real system.
        let opts = Opts {
            keep_cwd: false,
            user: Some(OsString::from("4294967294")),
            command: vec![],
        };
        let v = build_run0_argv(&opts);
        assert!(
            !v.contains(&OsString::from("--chdir")),
            "expected no --chdir for nonexistent user, got: {v:?}"
        );
    }

    #[test]
    fn argv_always_sets_pkexec_uid() {
        let opts = Opts {
            keep_cwd: true,
            user: None,
            command: vec![],
        };
        let v = build_run0_argv(&opts);
        let pos = v.iter().position(|a| a == "--setenv");
        assert!(pos.is_some(), "--setenv must be present");
        let val = &v[pos.unwrap() + 1];
        assert!(
            val.to_string_lossy().starts_with("PKEXEC_UID="),
            "expected PKEXEC_UID=…, got {val:?}"
        );
    }

    /// The PKEXEC_UID value must equal the real UID of the calling process.
    #[test]
    fn argv_pkexec_uid_matches_current_uid() {
        let opts = Opts {
            keep_cwd: true,
            user: None,
            command: vec![],
        };
        let v = build_run0_argv(&opts);
        let pos = v.iter().position(|a| a == "--setenv").unwrap();
        let got = v[pos + 1].to_str().unwrap();
        let expected = format!("PKEXEC_UID={}", current_uid());
        assert_eq!(got, expected);
    }

    /// Numeric-UID user strings must be forwarded verbatim to run0.
    #[test]
    fn argv_numeric_uid_user_forwarded_verbatim() {
        let opts = Opts {
            keep_cwd: true,
            user: Some(OsString::from("1000")),
            command: vec![OsString::from("id")],
        };
        let v = build_run0_argv(&opts);
        assert_eq!(v[0], "--user");
        assert_eq!(v[1], "1000");
    }

    /// #UID-prefixed user strings must also be forwarded verbatim.
    #[test]
    fn argv_hash_uid_user_forwarded_verbatim() {
        let opts = Opts {
            keep_cwd: true,
            user: Some(OsString::from("#0")),
            command: vec![OsString::from("whoami")],
        };
        let v = build_run0_argv(&opts);
        assert_eq!(v[1], "#0");
    }

    /// --via-shell must be absent whenever a command is provided.
    #[test]
    fn argv_no_via_shell_when_command_present() {
        let opts = Opts {
            keep_cwd: true,
            user: None,
            command: vec![OsString::from("ls")],
        };
        let v = build_run0_argv(&opts);
        assert!(!v.contains(&OsString::from("--via-shell")));
    }

    /// --user must precede --setenv which must precede the command.
    #[test]
    fn argv_stable_argument_order() {
        let opts = Opts {
            keep_cwd: true,
            user: Some(OsString::from("root")),
            command: vec![OsString::from("id")],
        };
        let v = build_run0_argv(&opts);
        let user_pos = v.iter().position(|a| a == "--user").unwrap();
        let setenv_pos = v.iter().position(|a| a == "--setenv").unwrap();
        let cmd_pos = v.iter().position(|a| a == "id").unwrap();
        assert!(user_pos < setenv_pos, "--user must precede --setenv");
        assert!(setenv_pos < cmd_pos, "--setenv must precede the command");
    }

    #[test]
    fn argv_command_forwarded_verbatim() {
        let opts = Opts {
            keep_cwd: true,
            user: None,
            command: vec![
                OsString::from("bash"),
                OsString::from("-c"),
                OsString::from("echo hello"),
            ],
        };
        let v = build_run0_argv(&opts);
        let pos = v.iter().position(|a| a == "bash").expect("bash not found");
        assert_eq!(v[pos + 1], "-c");
        assert_eq!(v[pos + 2], "echo hello");
    }

    // ── lookup_target_home ────────────────────────────────────────────────────

    #[test]
    fn lookup_target_home_by_name_root() {
        assert!(lookup_target_home(&OsString::from("root")).is_some());
    }

    #[test]
    fn lookup_target_home_by_numeric_uid_zero() {
        assert!(lookup_target_home(&OsString::from("0")).is_some());
    }

    #[test]
    fn lookup_target_home_by_hash_uid_zero() {
        assert!(lookup_target_home(&OsString::from("#0")).is_some());
    }

    /// All three spellings of root (name / plain UID / #UID) must resolve to
    /// the same home directory.
    #[test]
    fn lookup_target_home_all_root_spellings_agree() {
        let by_name = lookup_target_home(&OsString::from("root"));
        let by_uid = lookup_target_home(&OsString::from("0"));
        let by_hash_uid = lookup_target_home(&OsString::from("#0"));
        assert_eq!(by_name, by_uid);
        assert_eq!(by_uid, by_hash_uid);
    }

    #[test]
    fn lookup_target_home_nonexistent_returns_none() {
        let home = lookup_target_home(&OsString::from("thisuserdoesnotexist_run0shimtest_xyzzy"));
        assert!(home.is_none());
    }

    // ── passwd lookups (live, best-effort) ────────────────────────────────────

    #[test]
    fn lookup_root_home_by_name() {
        let home = lookup_home_by_name(OsStr::new("root"));
        assert!(home.is_some(), "lookup of 'root' by name should succeed");
        assert!(!home.unwrap().is_empty(), "root home should not be empty");
    }

    #[test]
    fn lookup_root_home_by_uid() {
        let home = lookup_home_by_uid(0);
        assert!(home.is_some(), "lookup of uid 0 should succeed");
        assert!(!home.unwrap().is_empty(), "uid-0 home should not be empty");
    }

    #[test]
    fn lookup_name_and_uid_agree_for_root() {
        assert_eq!(
            lookup_home_by_name(OsStr::new("root")),
            lookup_home_by_uid(0),
        );
    }

    #[test]
    fn lookup_nonexistent_user_returns_none() {
        assert!(
            lookup_home_by_name(OsStr::new("thisuserdoesnotexist_run0shimtest_xyzzy")).is_none()
        );
    }

    #[test]
    fn lookup_very_high_uid_returns_none() {
        assert!(lookup_home_by_uid(u32::MAX).is_none());
    }

    #[test]
    fn parse_double_dash_stops_option_parsing() {
        let opts = parse_args_from(args(&["--keep-cwd", "--", "echo", "--keep-cwd"]));
        assert!(opts.keep_cwd);
        assert_eq!(
            opts.command,
            vec![OsString::from("echo"), OsString::from("--keep-cwd")]
        );
    }

    #[test]
    fn uid_spec_hash_only_returns_none() {
        assert_eq!(parse_uid_spec(&OsString::from("#")), None);
    }
}
