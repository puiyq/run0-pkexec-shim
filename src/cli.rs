use crate::VERSION;
use std::{env, ffi::OsString, process::exit};
/// Parsed representation of the pkexec command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opts {
    pub keep_cwd: bool,
    pub user: Option<OsString>,
    pub run0_args: Vec<OsString>,
    pub command: Vec<OsString>,
}

/// Prints the help message to stdout.
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

    --run0-extra-arg <ARG>
        an extra argument to pass to run0 (can be specified multiple times)

    --help
        show this help message

    --version
        show version information
"#
    );
}

/// Parses `std::env::args_os()` into [`Opts`].
pub fn parse_args() -> Opts {
    parse_args_from(env::args_os())
}

/// Parses an arbitrary iterator of `OsString` arguments into [`Opts`].
///
/// The first element is consumed as the program name (argv\[0\]) and ignored;
/// subsequent elements are parsed as options and the target command.
pub fn parse_args_from<I>(argv: I) -> Opts
where
    I: IntoIterator<Item = OsString>,
{
    let mut argv = argv.into_iter();
    let _ = argv.next();

    let mut keep_cwd = false;
    let mut user = None;
    let mut run0_args = Vec::new();
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
            Some("--run0-extra-arg") => {
                let v = argv.next().unwrap_or_else(|| {
                    eprintln!("pkexec: missing --run0-extra-arg value");
                    exit(2);
                });
                run0_args.push(v);
            }
            Some(s) if s.starts_with("--run0-extra-arg=") => {
                run0_args.push(OsString::from(s.strip_prefix("--run0-extra-arg=").unwrap()));
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
        run0_args,
        command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn args(argv: &[&str]) -> impl Iterator<Item = OsString> {
        let mut v: Vec<OsString> = vec![OsString::from("pkexec")];
        v.extend(argv.iter().map(OsString::from));
        v.into_iter()
    }

    #[test]
    fn parse_empty_args() {
        let opts = parse_args_from(args(&[]));
        assert!(!opts.keep_cwd);
        assert!(opts.user.is_none());
        assert!(opts.run0_args.is_empty());
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
    #[test]
    fn parse_user_equals_form_with_embedded_equals() {
        let opts = parse_args_from(args(&["--user=a=b"]));
        assert_eq!(opts.user.as_deref(), Some(OsStr::new("a=b")));
    }
    #[test]
    fn parse_user_last_occurrence_wins() {
        let opts = parse_args_from(args(&["--user", "alice", "--user", "bob"]));
        assert_eq!(opts.user.as_deref(), Some(OsStr::new("bob")));
    }
    #[test]
    fn parse_run0_extra_arg_space_separated() {
        let opts = parse_args_from(args(&["--run0-extra-arg", "--background"]));
        assert_eq!(opts.run0_args, vec![OsString::from("--background")]);
    }
    #[test]
    fn parse_run0_extra_arg_equals_form() {
        let opts = parse_args_from(args(&["--run0-extra-arg=--timeout=5"]));
        assert_eq!(opts.run0_args, vec![OsString::from("--timeout=5")]);
    }
    #[test]
    fn parse_run0_extra_arg_multiple() {
        let opts = parse_args_from(args(&[
            "--run0-extra-arg",
            "--background",
            "--run0-extra-arg=--timeout=5",
        ]));
        assert_eq!(
            opts.run0_args,
            vec![
                OsString::from("--background"),
                OsString::from("--timeout=5")
            ]
        );
    }
    #[test]
    fn parse_command_after_options() {
        let opts = parse_args_from(args(&["--user", "alice", "ls", "-la", "/tmp"]));
        assert_eq!(
            opts.command,
            vec![
                OsString::from("ls"),
                OsString::from("-la"),
                OsString::from("/tmp")
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
        let opts = parse_args_from(args(&[
            "--keep-cwd",
            "--user=carol",
            "--run0-extra-arg=--nice=10",
            "env",
        ]));
        assert!(opts.keep_cwd);
        assert_eq!(opts.user.as_deref(), Some(OsStr::new("carol")));
        assert_eq!(opts.run0_args, vec![OsString::from("--nice=10")]);
        assert_eq!(opts.command, vec![OsString::from("env")]);
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
    fn parse_empty_user_equals_form() {
        // `--user=` with an empty value is accepted as Some("") and forwarded
        // verbatim; validation of the user name is left to run0.
        let opts = parse_args_from(args(&["--user="]));
        assert_eq!(opts.user.as_deref(), Some(OsStr::new("")));
    }

    #[test]
    fn parse_run0_extra_arg_equals_with_embedded_equals() {
        let opts = parse_args_from(args(&["--run0-extra-arg=--property=a=b"]));
        assert_eq!(opts.run0_args, vec![OsString::from("--property=a=b")]);
    }
}
