mod config;
mod mount;
mod mount_function;
mod program;
mod utils;

fn main() {
    use itertools::Itertools;
    use nix::unistd;
    use std::env;
    use std::path::PathBuf;
    use std::process::exit;

    let old_ids: (unistd::Uid, unistd::Gid) = (unistd::getuid(), unistd::getgid());
    unistd::setuid(0.into()).expect("Could not change to root user");
    unistd::setgid(0.into()).expect("Could not change to root group");

    let args = env::args().collect_vec();
    if args.len() < 3 {
        eprintln!("Usage: {} <config_name> <program> [args...]", args[0]);
        exit(1);
    }

    mount::unshare_and_privatise_mounts();
    let config = config::parse_config(&PathBuf::from(&args[1])).expect("Failed to get config");
    mount::mount(config).expect("Failed to mount files");

    program::replace_with_program(
        std::path::PathBuf::from(&args[2]),
        &args[2..],
        old_ids.0,
        old_ids.1,
    );
}
