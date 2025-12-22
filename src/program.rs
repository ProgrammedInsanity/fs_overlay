pub fn replace_with_program(
    program_path: std::path::PathBuf,
    program_args: &[String],
    new_uid: nix::unistd::Uid,
    new_gid: nix::unistd::Gid,
) -> ! {
    use crate::utils::open_program;
    use nix::unistd;
    use std::ffi::CString;
    use std::str::FromStr;

    let program_fd =
        open_program(&program_path).expect("Failed to get valid file descriptor for program");

    let args: Vec<CString> = program_args
        .iter()
        .map(|s| CString::new(s.clone()).expect("Invalid argument"))
        .collect();

    unistd::setgid(new_gid).expect("Could not change group to user");
    unistd::setuid(new_uid).expect("Could not change user to user");
    if cfg!(debug_assertions) {
        println!("eids: u:{}, g: {}", unistd::geteuid(), unistd::getegid());
        println!("ids: u:{}, g: {}", unistd::getuid(), unistd::getgid());
    }

    let mut envs = std::env::vars()
        .map(|(mut key, val)| {
            key.push('=');
            key.push_str(&val);
            CString::from_str(&key)
        })
        .collect::<Result<Vec<CString>, _>>()
        .expect("Failed to convert String to Cstring");

    envs.insert(
        0,
        CString::from_str(
            program_path
                .file_name()
                .unwrap()
                .to_str()
                .expect("Expected valid utf8"),
        )
        .expect("Failed to convert file name str to Cstring"),
    );

    // program_fd must not have been opened with O_CLOEXEC
    unistd::fexecve(program_fd, &args, &envs).unwrap();
    unreachable!()
}
