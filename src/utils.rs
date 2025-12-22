#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Opening file {path:?} errored with {source}")]
    FileOpen {
        path: std::path::PathBuf,
        source: nix::errno::Errno,
    },
    #[error("The owner or group of {0:?} is not root")]
    OwnerGroupNotRoot(std::path::PathBuf),
    #[error("The permissions for 'other' on {0:?} are not 0 or 4 (must be readable, not writable)")]
    IncorrectPermissions(std::path::PathBuf),
    #[error("The path {0:?} is not a regular file")]
    NotRegularFile(std::path::PathBuf),
    #[error("Directory {0:?} has special bits set (setuid, setgid, or sticky)")]
    SpecialBitsOnFD(std::path::PathBuf),
    #[error("Path {0:?} is supposed to be a directory but is not")]
    NotDirectory(std::path::PathBuf),
    #[error("Path {0:?} is too short")]
    PathTooShort(std::path::PathBuf),
    #[error("Invalid path component {component} in path {path:?}")]
    InvalidPathComponent {
        path: std::path::PathBuf,
        component: String,
    },
    #[error("Error while validating directory {path:?} with error {error}")]
    DirectoryValidation {
        path: std::path::PathBuf,
        error: Box<Error>,
    },
    #[error("Path {0:?} is not a valid target directory")]
    InvalidTargetDir(std::path::PathBuf),
    #[error("Path {0:?} is not a valid source path (must be under /nix/store/store-object/)")]
    InvalidSourcePath(std::path::PathBuf),
    #[error("Entry name {0:?} is not valid UTF-8")]
    NonUtf8EntryName(std::ffi::OsString),
}
static ALLOWED_TARGET_DIRS: std::sync::LazyLock<[&'static std::ffi::OsStr; 7]> =
    std::sync::LazyLock::new(|| {
        [
            std::ffi::OsStr::new("lib"),
            std::ffi::OsStr::new("lib32"),
            std::ffi::OsStr::new("lib64"),
            std::ffi::OsStr::new("libexec"),
            std::ffi::OsStr::new("bin"),
            std::ffi::OsStr::new("sbin"),
            std::ffi::OsStr::new("include"),
        ]
    });

#[derive(PartialEq, Eq)]
pub enum OpenFileOverrideType {
    Source,
    Target,
}

#[derive(PartialEq, Eq)]
pub enum DirType {
    Source,
    Target,
    ParentOfSourceOverrideFile,
    ParentOfTargetOverrideFile,
}

fn get_fd_path<F: std::os::fd::AsRawFd>(fd: &F) -> std::path::PathBuf {
    std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd()))
        .unwrap_or_else(|_| std::path::PathBuf::from("unknown"))
}

fn openat_dir(
    fd: &nix::dir::Dir,
    dir_name: &std::path::Component,
    permission_mask: nix::libc::mode_t,
    group_must_be_root: bool,
) -> Result<nix::dir::Dir, Error> {
    use nix::fcntl::OFlag;
    use nix::fcntl::openat;
    use nix::libc::{S_IFDIR, S_IFMT, S_ISGID, S_ISUID};
    use nix::sys::stat::Mode;
    use nix::sys::stat::fstat;
    use std::path::Component;

    let dir_path = get_fd_path(fd);
    let dir_name_os = match dir_name {
        Component::Normal(dir_name) => *dir_name,
        comp => {
            return Err(Error::InvalidPathComponent {
                path: dir_path,
                component: format!("{comp:?}"),
            });
        }
    };

    let dir = openat(
        fd,
        dir_name_os,
        OFlag::O_NOFOLLOW | OFlag::O_RDONLY,
        Mode::empty(),
    )
    .map_err(|e| Error::FileOpen {
        path: dir_path.join(dir_name_os),
        source: e,
    })?;
    let stats = fstat(&dir).map_err(|e| Error::FileOpen {
        path: dir_path.join(dir_name_os),
        source: e,
    })?;

    if stats.st_mode & S_IFMT != S_IFDIR {
        return Err(Error::NotDirectory(get_fd_path(&dir)));
    }

    if stats.st_uid != 0 {
        return Err(Error::OwnerGroupNotRoot(get_fd_path(&dir)));
    }

    if group_must_be_root && stats.st_gid != 0 {
        return Err(Error::OwnerGroupNotRoot(get_fd_path(&dir)));
    }

    // Write by others is not allowed.
    if stats.st_mode & 0b010 == 2 {
        return Err(Error::IncorrectPermissions(get_fd_path(&dir)));
    }

    // These are never allowed
    if (stats.st_mode & (S_ISGID | S_ISUID)) != 0 {
        return Err(Error::SpecialBitsOnFD(get_fd_path(&dir)));
    }

    // Allow caller to test for extra permission
    if (stats.st_mode & permission_mask) != 0 {
        return Err(Error::IncorrectPermissions(get_fd_path(&dir)));
    }

    // Only errors when fd is not a dir but it is a dir so we unwrap.
    Ok(nix::dir::Dir::from_fd(dir).unwrap())
}

fn openat_file(
    fd: &nix::dir::Dir,
    file_name: &std::path::Component,
    flags: nix::fcntl::OFlag,
) -> Result<std::fs::File, Error> {
    use nix::fcntl::OFlag;
    use nix::fcntl::openat;
    use nix::libc::{S_IFMT, S_IFREG, S_ISGID, S_ISUID, S_ISVTX};
    use nix::sys::stat::Mode;
    use nix::sys::stat::fstat;
    use std::fs::File;
    use std::path::Component;

    let dir_path = get_fd_path(fd);
    let file_name_os = match file_name {
        Component::Normal(file_name) => file_name,
        comp => {
            return Err(Error::InvalidPathComponent {
                path: dir_path,
                component: format!("{comp:?}"),
            });
        }
    };

    let file = openat(
        fd,
        *file_name_os,
        flags | OFlag::O_RDONLY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|e| Error::FileOpen {
        path: dir_path.join(file_name_os),
        source: e,
    })?;
    let stats = fstat(&file).map_err(|e| Error::FileOpen {
        path: dir_path.join(file_name_os),
        source: e,
    })?;

    // Source and target files must be owned by root:root.
    if stats.st_uid != 0 || stats.st_gid != 0 {
        return Err(Error::OwnerGroupNotRoot(get_fd_path(&file)));
    }

    // Write by others is not allowed. And must be readable by others.
    if stats.st_mode & 0b010 == 2 || stats.st_mode & 0b100 != 4 {
        return Err(Error::IncorrectPermissions(get_fd_path(&file)));
    }

    if stats.st_mode & S_IFMT != S_IFREG {
        return Err(Error::NotRegularFile(get_fd_path(&file)));
    }

    // No special bits allowed on source or target files.
    if (stats.st_mode & (S_ISUID | S_ISGID | S_ISVTX)) != 0 {
        return Err(Error::SpecialBitsOnFD(get_fd_path(&file)));
    }

    Ok(File::from(file))
}

pub fn open_root() -> Result<nix::dir::Dir, Error> {
    use nix::fcntl::OFlag;
    use nix::fcntl::open;
    use nix::libc::{S_IFDIR, S_IFMT, S_ISGID, S_ISUID, S_ISVTX};
    use nix::sys::stat::Mode;
    use nix::sys::stat::fstat;

    let dir = open(
        "/",
        OFlag::O_NOFOLLOW | OFlag::O_RDONLY | OFlag::O_DIRECTORY,
        Mode::empty(),
    )
    .map_err(|e| Error::FileOpen {
        path: std::path::PathBuf::from("/"),
        source: e,
    })?;
    let stats = fstat(&dir).map_err(|e| Error::FileOpen {
        path: std::path::PathBuf::from("/"),
        source: e,
    })?;

    if stats.st_uid != 0 || stats.st_gid != 0 {
        return Err(Error::OwnerGroupNotRoot(get_fd_path(&dir)));
    }

    if stats.st_mode & 0b010 == 2 {
        return Err(Error::IncorrectPermissions(get_fd_path(&dir)));
    }

    if (stats.st_mode & (S_ISUID | S_ISGID | S_ISVTX)) != 0 {
        return Err(Error::SpecialBitsOnFD(get_fd_path(&dir)));
    }

    if stats.st_mode & S_IFMT != S_IFDIR {
        return Err(Error::NotDirectory(get_fd_path(&dir)));
    }

    // Only errors when fd is not a dir but it is a dir so we unwrap.
    Ok(nix::dir::Dir::from_fd(dir).unwrap())
}

pub fn open_dir(path: &std::path::Path, dir_type: &DirType) -> Result<nix::dir::Dir, Error> {
    use nix::libc::{S_ISGID, S_ISUID, S_ISVTX};
    use nix::sys::stat::Mode;
    use nix::sys::stat::mkdirat;
    use std::path::Component;

    let components: Vec<Component> = path.components().collect();
    // sanity check is enforced by config that checks .is_absolute
    assert_eq!(components.first(), Some(&Component::RootDir));

    match dir_type {
        DirType::Target | DirType::ParentOfTargetOverrideFile => match components.get(1) {
            Some(Component::Normal(name)) => {
                if !ALLOWED_TARGET_DIRS.contains(name) {
                    return Err(Error::InvalidTargetDir(path.to_path_buf()));
                }
            }
            _ => return Err(Error::InvalidTargetDir(path.to_path_buf())),
        },
        DirType::Source | DirType::ParentOfSourceOverrideFile => {
            // We don't have the check for `..` it is later ensured that it doesn't have those.
            use std::ffi::OsStr;
            let start = [
                Component::RootDir,
                Component::Normal(OsStr::new("nix")),
                Component::Normal(OsStr::new("store")),
            ];
            if components.len() < 3 || components[..3] != start {
                return Err(Error::InvalidSourcePath(path.to_path_buf()));
            }
            // ensure that it is in a store object `/nix/store/store-object/...`
            if components[2..].len() <= 1 {
                return Err(Error::InvalidSourcePath(path.to_path_buf()));
            }
        }
    }

    let mut dir_fd = open_root()?;

    // Permission mask defines bits that are NOT allowed.
    let permission_mask =
        if *dir_type == DirType::Source || *dir_type == DirType::ParentOfSourceOverrideFile {
            // Sources: Ancestors must not have special permissions bits set except sticky bit.
            S_ISUID | S_ISGID
        } else {
            // Targets: All ancestor directories must have no special permission bits.
            S_ISUID | S_ISGID | S_ISVTX
        };

    for (i, comp) in components[1..].iter().enumerate() {
        match comp {
            Component::Normal(name) => {
                // Direct parent of source must not have any special permissions bits set (including sticky bit).
                let permission_mask = permission_mask
                    | (if *dir_type == DirType::Source && components[1..].len() - i <= 2 {
                        S_ISVTX
                    } else {
                        0
                    })
                    | (if *dir_type == DirType::ParentOfSourceOverrideFile
                        && components[1..].len() - i == 1
                    {
                        S_ISVTX
                    } else {
                        0
                    });

                // Targets must be owned by root:root.
                // Sources: Ancestors must be owned by root:anygroup, but direct parent must be root:root.
                let group_must_be_root = *dir_type == DirType::Target
                    || (components[1..].len() - i <= 2 && *dir_type == DirType::Source)
                    || *dir_type == DirType::ParentOfSourceOverrideFile
                        && components[1..].len() - i == 1;

                match openat_dir(&dir_fd, comp, permission_mask, group_must_be_root) {
                    Ok(fd) => dir_fd = fd,
                    Err(Error::FileOpen {
                        source: nix::errno::Errno::ENOENT,
                        ..
                    }) if *dir_type == DirType::Target => {
                        // If the target directory does not exist, it is created with permissions 755 and ownership root:root.
                        // (rwxr-xr-x)
                        let mode = Mode::from_bits_truncate(0o755);
                        mkdirat(&dir_fd, *name, mode).map_err(|e| Error::FileOpen {
                            path: get_fd_path(&dir_fd).join(name),
                            source: e,
                        })?;
                        dir_fd = openat_dir(&dir_fd, comp, permission_mask, group_must_be_root)?;
                    }
                    Err(e) => return Err(e),
                }
            }
            Component::RootDir => {
                unreachable!()
            }
            comp => {
                return Err(Error::InvalidPathComponent {
                    path: path.to_path_buf(),
                    component: format!("{comp:?}"),
                });
            }
        }
    }

    if *dir_type == DirType::Source {
        validate_dir_content(&mut dir_fd)?;
    }

    Ok(dir_fd)
}

pub fn open_override_file(
    path: &std::path::Path,
    flags: nix::fcntl::OFlag,
    override_file_type: &OpenFileOverrideType,
) -> Result<std::fs::File, Error> {
    use std::path::Component;

    let components: Vec<Component> = path.components().collect();
    // sanity check is enforced by config that checks .is_absolute
    assert_eq!(components.first(), Some(&Component::RootDir));
    if components.len() < 2 {
        return Err(Error::PathTooShort(path.to_path_buf()));
    }

    let dir_fd = open_dir(
        path.parent().unwrap(),
        match override_file_type {
            OpenFileOverrideType::Source => &DirType::ParentOfSourceOverrideFile,
            OpenFileOverrideType::Target => &DirType::ParentOfTargetOverrideFile,
        },
    )?;

    openat_file(&dir_fd, components.last().unwrap(), flags)
}

pub fn open_program(path: &std::path::Path) -> Result<std::fs::File, Error> {
    use nix::fcntl::OFlag;
    // The program should follow the same rules as a source of a file override so we will reuse it.
    open_override_file(path, OFlag::empty(), &OpenFileOverrideType::Source)
}

pub fn open_config_file(file_name: &std::path::Component) -> Result<std::fs::File, Error> {
    use nix::fcntl::OFlag;
    use nix::libc::{S_ISGID, S_ISUID, S_ISVTX};
    use std::path::{Component, PathBuf};

    let path = PathBuf::from("/etc/fs_overlay");
    let components: Vec<Component> = path.components().collect();

    let mut dir_fd = open_root()?;

    for comp in &components[1..] {
        match comp {
            Component::Normal(_) => {
                // Ancestors must be owned by root:root and have no special permission bits.
                dir_fd = openat_dir(&dir_fd, comp, S_ISUID | S_ISGID | S_ISVTX, true)?;
            }
            Component::RootDir => {
                unreachable!()
            }
            comp => {
                return Err(Error::InvalidPathComponent {
                    path: path.clone(),
                    component: format!("{comp:?}"),
                });
            }
        }
    }

    openat_file(&dir_fd, file_name, OFlag::empty())
}

pub fn validate_dir_content(dir_fd: &mut nix::dir::Dir) -> Result<(), Error> {
    use nix::fcntl::AtFlags;
    use nix::fcntl::OFlag;
    use nix::fcntl::{openat, readlinkat};
    use nix::libc::{S_IFLNK, S_IFMT, S_ISGID, S_ISUID, S_ISVTX};

    use nix::sys::stat::Mode;
    use nix::sys::stat::fstatat;
    use std::ffi::OsStr;
    use std::path::{Component, PathBuf};

    let dir_path = get_fd_path(dir_fd);
    let contents: Vec<Result<nix::dir::Entry, nix::errno::Errno>> = dir_fd.iter().collect();
    let dir_fd = &*dir_fd;

    for dir_entry in contents {
        let dir_entry = dir_entry.map_err(|e| Error::FileOpen {
            path: dir_path.clone(),
            source: e,
        })?;
        let entry_name_c = dir_entry.file_name();
        let entry_name = entry_name_c.to_str().map_err(|_| {
            use std::os::unix::ffi::OsStrExt;
            Error::NonUtf8EntryName(std::ffi::OsStr::from_bytes(entry_name_c.to_bytes()).to_owned())
        })?;

        if entry_name == "." || entry_name == ".." {
            continue;
        }

        let entry_name_comp = Component::Normal(OsStr::new(entry_name));

        let entry_fd = openat(
            dir_fd,
            entry_name,
            OFlag::O_PATH | OFlag::O_NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|e| Error::FileOpen {
            path: dir_path.join(entry_name),
            source: e,
        })?;
        let entry_stats = fstatat(
            &entry_fd,
            "",
            AtFlags::AT_SYMLINK_NOFOLLOW | AtFlags::AT_EMPTY_PATH,
        )
        .map_err(|e| Error::FileOpen {
            path: dir_path.join(entry_name),
            source: e,
        })?;

        // Symlinks in the source directories must point to a file that follow the same rules as sources in file overrides.
        // Therefore we call readlinkat and then open_override_file with OpenFileOverrideType::Source.
        if entry_stats.st_mode & S_IFMT == S_IFLNK
            && entry_stats.st_uid == 0
            && entry_stats.st_gid == 0
        {
            let path = readlinkat(&entry_fd, "").map_err(|e| Error::FileOpen {
                path: dir_path.join(entry_name),
                source: e,
            })?;
            
            if cfg!(debug_assertions) {
                println!("verifying correctness of symlink {}", path.display());
            }

            open_override_file(
                &PathBuf::from(path),
                OFlag::empty(),
                &OpenFileOverrideType::Source,
            )
            .map_err(|err| Error::DirectoryValidation {
                path: dir_path.join(entry_name),
                error: Box::new(err),
            })?;
            continue;
        }

        match openat_dir(dir_fd, &entry_name_comp, S_ISGID | S_ISUID | S_ISVTX, true) {
            Err(Error::NotDirectory(_)) => {
                // All contained files must follow the same rules as sources in file overrides.
                // The ancestors permissions and etc of the dir entry are already verified so we can just call `openat_file`
                openat_file(dir_fd, &entry_name_comp, OFlag::empty()).map(|_| ())
            }
            Ok(mut dir) => validate_dir_content(&mut dir),
            Err(other) => Err(other),
        }
        .map_err(|err| Error::DirectoryValidation {
            path: dir_path.join(entry_name),
            error: Box::new(err),
        })?;
    }

    Ok(())
}
