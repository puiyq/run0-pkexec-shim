//! Entry point for the `pkexec` shim binary.
//!
//! Parses arguments, builds the `run0` command line, then **exec**s into it,
//! replacing the current process image.  This means:
//!
//! - Signals sent to the shim PID are delivered directly to run0.
//! - No extra process appears in the process tree.
//! - The exit code is run0's own exit code, without any mapping layer.
use run0_pkexec_shim::*;
use std::os::unix::process::CommandExt; // exec()
use std::process::{Command, exit};

pub fn build_run0_command(opts: &Opts) -> Command {
    let argv = build_run0_argv(opts);
    let mut cmd = Command::new(run0_bin());
    cmd.args(&argv);
    cmd
}

fn main() {
    let opts = parse_args();
    let mut cmd = build_run0_command(&opts);

    let err = cmd.exec();
    eprintln!("pkexec: failed to execute run0: {err}");
    exit(1);
}
