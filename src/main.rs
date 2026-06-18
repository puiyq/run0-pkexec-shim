pub const VERSION: &str = env!("CARGO_PKG_VERSION");

use std::os::unix::process::CommandExt;
use std::process::Command;

mod cli;
mod run0;
mod user;

fn main() {
    let opts = cli::parse_args();
    let bin = run0::run0_bin();
    let argv = run0::build_run0_argv(&opts);

    // Replace the current process with the constructed run0 invocation.
    let err = Command::new(&bin).args(&argv).exec();

    eprintln!("pkexec: failed to execute {:?}: {}", bin, err);
    std::process::exit(1);
}
