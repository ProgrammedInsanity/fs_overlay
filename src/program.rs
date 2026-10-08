pub fn replace_with_program(
    program_path: std::path::PathBuf,
    program_args: &[String],
    new_uid: nix::unistd::Uid,
    new_gid: nix::unistd::Gid,
) -> ! {
    use nix::unistd;
    use std::ffi::CString;
    use std::str::FromStr;

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

    let envs = std::env::vars()
        .map(|(mut key, val)| {
            key.push('=');
            key.push_str(&val);
            CString::from_str(&key)
        })
        .collect::<Result<Vec<CString>, _>>()
        .expect("Failed to convert String to Cstring");

    unistd::execvpe(
        &CString::from_str(program_path.to_str().unwrap()).unwrap(),
        &args,
        &envs,
    )
    .unwrap();
    unreachable!()
}
