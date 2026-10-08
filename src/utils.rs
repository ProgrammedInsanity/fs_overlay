use std::path::{Path, PathBuf};

use nix::errno::Errno::ELOOP;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Opening file {path:?} errored with {source}")]
    FileOpen {
        path: std::path::PathBuf,
        source: nix::errno::Errno,
    },
    #[error("The owner of {0:?} is not root")]
    OwnerNotRoot(std::path::PathBuf),
    #[error("The group of {0:?} is not root")]
    GroupNotRoot(std::path::PathBuf),
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
    #[error("Path {0:?} is not a valid source path (must be under /nix/store/store-object/)")]
    InvalidSourcePath(std::path::PathBuf),
    #[error("Entry name {0:?} is not valid UTF-8")]
    NonUtf8EntryName(std::ffi::OsString),
    #[error("Path {0:?} is not absolute")]
    PathNotAbsolute(std::path::PathBuf),
    #[error("Path {0:?} contains a prefix component")]
    PathHasPrefix(std::path::PathBuf),
    #[error("Symlink resolution for {0:?} exceeded the maximum depth of {1}")]
    SymlinkDepthExceeded(std::path::PathBuf, u32),
    #[error("Path {0:?} is neither a regular file, a directory nor a symlink (type {1:o})")]
    InvalidFileType(std::path::PathBuf, u64),
    #[error("Path {0:?} is not a symlink")]
    NotSymlink(std::path::PathBuf),
}

/// SAFETY: Should not be used for anything other than logging
fn get_fd_path<F: std::os::fd::AsRawFd>(fd: &F) -> std::path::PathBuf {
    std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd()))
        .unwrap_or_else(|_| std::path::PathBuf::from("unknown"))
}

/// Entries we mount must be owned by root, and for directories by the root group.
fn check_root_owned(
    path: &Path,
    stats: &nix::sys::stat::FileStat,
    group_must_be_root: bool,
) -> Result<(), Error> {
    if stats.st_uid != 0 {
        return Err(Error::OwnerNotRoot(path.to_path_buf()));
    }

    if group_must_be_root && stats.st_gid != 0 {
        return Err(Error::GroupNotRoot(path.to_path_buf()));
    }

    Ok(())
}

/// "other" may only read, never write, and `special_bits` (setuid/setgid/sticky) are never allowed.
fn check_permission_bits(
    path: &Path,
    stats: &nix::sys::stat::FileStat,
    special_bits: nix::libc::mode_t,
) -> Result<(), Error> {
    if stats.st_mode & 0b010 != 0 {
        return Err(Error::IncorrectPermissions(path.to_path_buf()));
    }

    if stats.st_mode & special_bits != 0 {
        return Err(Error::SpecialBitsOnFD(path.to_path_buf()));
    }

    Ok(())
}

fn openat_dir(
    fd: &nix::dir::Dir,
    dir_name: &std::path::Component,
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
    let path = get_fd_path(&dir);
    let stats = fstat(&dir).map_err(|e| Error::FileOpen {
        path: path.clone(),
        source: e,
    })?;

    if stats.st_mode & S_IFMT != S_IFDIR {
        return Err(Error::NotDirectory(path));
    }
    if stats.st_mode & 0b100 == 0 {
        return Err(Error::IncorrectPermissions(path.to_path_buf()));
    }

    check_root_owned(&path, &stats, group_must_be_root)?;
    check_permission_bits(&path, &stats, S_ISGID | S_ISUID)?;

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
    use nix::libc::{S_IFMT, S_IFREG};
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
    let path = get_fd_path(&file);
    let stats = fstat(&file).map_err(|e| Error::FileOpen {
        path: path.clone(),
        source: e,
    })?;

    if stats.st_mode & S_IFMT != S_IFREG {
        return Err(Error::NotRegularFile(path));
    }

    check_root_owned(&path, &stats, true)?;
    check_permission_bits(&path, &stats, 0)?;

    Ok(File::from(file))
}

pub fn open_root() -> Result<nix::dir::Dir, Error> {
    use nix::fcntl::OFlag;
    use nix::fcntl::open;
    use nix::libc::{S_IFDIR, S_IFMT, S_ISGID, S_ISUID, S_ISVTX};
    use nix::sys::stat::Mode;
    use nix::sys::stat::fstat;

    let path = PathBuf::from("/");

    let dir = open(
        &path,
        OFlag::O_NOFOLLOW | OFlag::O_RDONLY | OFlag::O_DIRECTORY,
        Mode::empty(),
    )
    .map_err(|e| Error::FileOpen {
        path: path.clone(),
        source: e,
    })?;
    let stats = fstat(&dir).map_err(|e| Error::FileOpen {
        path: path.clone(),
        source: e,
    })?;

    if stats.st_mode & S_IFMT != S_IFDIR {
        return Err(Error::NotDirectory(path));
    }

    check_root_owned(&path, &stats, true)?;

    // Write by others is not allowed.
    if stats.st_mode & 0b010 != 0 {
        return Err(Error::IncorrectPermissions(path));
    }

    if stats.st_mode & (S_ISUID | S_ISGID | S_ISVTX) != 0 {
        return Err(Error::SpecialBitsOnFD(path));
    }

    // Only errors when fd is not a dir but it is a dir so we unwrap.
    Ok(nix::dir::Dir::from_fd(dir).unwrap())
}

fn open_dir(
    path: &std::path::Path,
    follow_sym_links: bool,
    depth: u32,
) -> Result<nix::dir::Dir, Error> {
    use nix::fcntl::AtFlags;
    use nix::fcntl::OFlag;
    use nix::fcntl::{openat, readlinkat};
    use nix::libc::{S_IFLNK, S_IFMT};
    use std::ffi::OsStr;
    use std::path::Component;

    use nix::sys::stat::Mode;
    use nix::sys::stat::fstatat;

    let components: Vec<Component> = path.components().collect();
    let mut curpath = PathBuf::from("/");
    let mut dir_fd = open_root()?;

    for (i, comp) in components[1..].iter().enumerate() {
        match comp {
            Component::Normal(name) => {
                curpath = curpath.join(name);
                // i== 2 is store:  [nix, store]
                let group_must_be_root = !(i == 1 && name == &OsStr::new("store"));
                // If the dir is a symlink then we need to resolve that but we still need to ensure that it follows the rules.
                // So if it is a symlink then feed the path back into resolve_and_validate_override and if that returns a dir use that
                dir_fd = match openat_dir(&dir_fd, comp, group_must_be_root) {
                    Ok(dir) => Ok(dir),
                    Err(Error::FileOpen {
                        path: errpath,
                        source: errsource,
                    }) => match errsource {
                        ELOOP => {
                            if !follow_sym_links {
                                return Err(Error::FileOpen {
                                    path: errpath,
                                    source: errsource,
                                });
                            }
                            let entry_fd = openat(
                                dir_fd,
                                *name,
                                OFlag::O_PATH | OFlag::O_NOFOLLOW,
                                Mode::empty(),
                            )
                            .map_err(|e| Error::FileOpen {
                                path: curpath.clone(),
                                source: e,
                            })?;
                            let entry_stats = fstatat(
                                &entry_fd,
                                "",
                                AtFlags::AT_SYMLINK_NOFOLLOW | AtFlags::AT_EMPTY_PATH,
                            )
                            .map_err(|e| Error::FileOpen {
                                path: curpath.clone(),
                                source: e,
                            })?;

                            if entry_stats.st_mode & S_IFMT == S_IFLNK {
                                let symlink_path =
                                    readlinkat(&entry_fd, "").map_err(|e| Error::FileOpen {
                                        path: curpath.clone(),
                                        source: e,
                                    })?;

                                let symlink_path = curpath.parent().unwrap().join(symlink_path);

                                let resolved =
                                    resolve_and_validate_override(&symlink_path, depth + 1)
                                        .map_err(|err| Error::DirectoryValidation {
                                            path: curpath.clone(),
                                            error: Box::new(err),
                                        })?;
                                match resolved {
                                    OV::Dir(dir) => Ok(dir),
                                    OV::File(_) => Err(Error::NotDirectory(curpath.clone())),
                                }
                            } else {
                                Err(Error::NotSymlink(curpath.clone()))
                            }
                        }
                        _ => Err(Error::FileOpen {
                            path: errpath,
                            source: errsource,
                        }),
                    },
                    Err(err) => Err(err),
                }?;
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

    Ok(dir_fd)
}

pub fn open_config_file(file_name: &std::path::Component) -> Result<std::fs::File, Error> {
    use nix::fcntl::OFlag;
    use std::path::{Component, PathBuf};

    let path = PathBuf::from("/etc/fs_overlay");
    let components: Vec<Component> = path.components().collect();

    let mut dir_fd = open_root()?;

    for comp in &components[1..] {
        match comp {
            Component::Normal(_) => {
                // Ancestors must be owned by root:root and have no special permission bits.
                dir_fd = openat_dir(&dir_fd, comp, true)?;
            }
            _ => {
                unreachable!()
            }
        }
    }

    openat_file(&dir_fd, file_name, OFlag::empty())
}

fn validate_dir_content(
    dir_fd: &mut nix::dir::Dir,
    dir_path: &Path,
    depth: u32,
) -> Result<(), Error> {
    use nix::fcntl::AtFlags;
    use nix::fcntl::OFlag;
    use nix::fcntl::{openat, readlinkat};
    use nix::libc::{S_IFLNK, S_IFMT};

    use nix::sys::stat::Mode;
    use nix::sys::stat::fstatat;
    use std::ffi::OsStr;
    use std::path::Component;

    let contents: Vec<Result<nix::dir::Entry, nix::errno::Errno>> = dir_fd.iter().collect();
    let dir_fd = &*dir_fd;

    for dir_entry in contents {
        let dir_entry = dir_entry.map_err(|e| Error::FileOpen {
            path: dir_path.to_path_buf(),
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

        if entry_stats.st_mode & S_IFMT == S_IFLNK {
            let symlink_path = readlinkat(&entry_fd, "").map_err(|e| Error::FileOpen {
                path: dir_path.join(entry_name),
                source: e,
            })?;

            let symlink_path = dir_path.join(symlink_path);

            resolve_and_validate_override(&symlink_path, depth + 1).map_err(|err| {
                Error::DirectoryValidation {
                    path: dir_path.join(entry_name),
                    error: Box::new(err),
                }
            })?;
            continue;
        }

        match openat_dir(dir_fd, &entry_name_comp, true) {
            Err(Error::NotDirectory(_)) => {
                // All contained files must follow the same rules as sources in file overrides.
                // The ancestors permissions and etc of the dir entry are already verified so we can just call `openat_file`
                openat_file(dir_fd, &entry_name_comp, OFlag::O_PATH).map(|_| ())
            }
            Ok(mut dir) => validate_dir_content(&mut dir, &dir_path.join(entry_name_comp), depth),
            Err(other) => Err(other),
        }
        .map_err(|err| Error::DirectoryValidation {
            path: dir_path.join(entry_name),
            error: Box::new(err),
        })?;
    }

    Ok(())
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            other => result.push(other),
        }
    }

    result
}

enum OV {
    File(std::fs::File),
    Dir(nix::dir::Dir),
}

fn check_path_shape(path: &Path) -> Result<(), Error> {
    use std::path::Component;

    if path.is_relative() {
        return Err(Error::PathNotAbsolute(path.to_path_buf()));
    }

    if path
        .components()
        .any(|comp| matches!(comp, Component::Prefix(_)))
    {
        return Err(Error::PathHasPrefix(path.to_path_buf()));
    }

    if path.components().count() <= 1 {
        return Err(Error::PathTooShort(path.to_path_buf()));
    }

    Ok(())
}

fn resolve_and_validate_override(path: &Path, depth: u32) -> Result<OV, Error> {
    use nix::fcntl::OFlag;
    use nix::fcntl::openat;
    use nix::fcntl::readlinkat;
    use nix::libc::{S_IFDIR, S_IFLNK, S_IFMT, S_IFREG, S_ISGID, S_ISUID};
    use nix::sys::stat::Mode;
    use nix::sys::stat::fstat;
    use std::fs::File;
    use std::path::Component;

    if depth > 42 {
        return Err(Error::SymlinkDepthExceeded(path.to_path_buf(), depth));
    }

    check_path_shape(path)?;

    let path = normalize_path(path);

    let components: Vec<Component> = path.components().collect();
    // sanity check
    assert_eq!(components.first(), Some(&Component::RootDir));

    use std::ffi::OsStr;

    let starts = [
        [
            Component::RootDir,
            Component::Normal(OsStr::new("run")),
            Component::Normal(OsStr::new("wrappers")),
        ],
        [
            Component::RootDir,
            Component::Normal(OsStr::new("nix")),
            Component::Normal(OsStr::new("store")),
        ],
    ];

    if components.len() < 3 || (components[..3] != starts[0] && components[..3] != starts[1]) {
        return Err(Error::InvalidSourcePath(path.to_path_buf()));
    }
    // ensure that it is in a store object `/nix/store/store-object/...`
    if components[2..].len() <= 1 {
        return Err(Error::InvalidSourcePath(path.to_path_buf()));
    }

    let dir_fd = open_dir(path.parent().unwrap(), true, depth)?;

    let fd = openat(
        &dir_fd,
        path.file_name().unwrap(),
        OFlag::O_PATH | OFlag::O_RDONLY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|e| Error::FileOpen {
        path: path.to_path_buf(),
        source: e,
    })?;
    let stats = fstat(&fd).map_err(|e| Error::FileOpen {
        path: path.to_path_buf(),
        source: e,
    })?;

    match stats.st_mode & S_IFMT {
        S_IFREG => {
            check_root_owned(&path, &stats, true)?;
            check_permission_bits(&path, &stats, 0)?;

            Ok(OV::File(File::from(fd)))
        }
        S_IFDIR => {
            check_root_owned(&path, &stats, true)?;
            check_permission_bits(&path, &stats, S_ISGID | S_ISUID)?;
            if stats.st_mode & 0b100 == 0 {
                return Err(Error::IncorrectPermissions(path.to_path_buf()));
            }

            // fd was opened with OFlag::O_PATH it cannot be read
            // Is this safe? idk ¯\_(?¿)_/¯
            let fd = openat(
                &fd,
                ".",
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
                Mode::empty(),
            )
            .unwrap();
            let mut fd = nix::dir::Dir::from_fd(fd).unwrap();

            validate_dir_content(&mut fd, &path, depth)?;

            Ok(OV::Dir(fd))
        }
        S_IFLNK => {
            check_root_owned(&path, &stats, true)?;

            let symlink_path = readlinkat(&fd, "").map_err(|e| Error::FileOpen {
                path: path.clone(),
                source: e,
            })?;
            let symlink_path = path.parent().unwrap().join(symlink_path);

            resolve_and_validate_override(&symlink_path, depth + 1)
        }
        _ => Err(Error::InvalidFileType(
            path,
            (stats.st_mode & S_IFMT) as u64,
        )),
    }
}

fn open_target_dir(path: &Path) -> Result<nix::dir::Dir, Error> {
    check_path_shape(path)?;

    open_dir(&normalize_path(path), false, 0)
}

fn open_target_file(path: &Path) -> Result<std::os::fd::OwnedFd, Error> {
    use nix::fcntl::OFlag;
    use nix::fcntl::openat;
    use nix::sys::stat::Mode;

    check_path_shape(path)?;
    let path = normalize_path(path);

    let dir_fd = open_dir(path.parent().unwrap(), false, 0)?;

    openat(
        &dir_fd,
        path.file_name().unwrap(),
        OFlag::O_PATH | OFlag::O_RDONLY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|e| Error::FileOpen {
        path: path.to_path_buf(),
        source: e,
    })
}

pub fn validate_override(
    source: PathBuf,
    target: PathBuf,
) -> Result<super::config::MntOverride, Error> {
    use super::config::DirOverride;
    use super::config::FileOverride;
    use super::config::MntOverride;

    // What a mess

    match resolve_and_validate_override(&source, 0) {
        Ok(OV::File(file)) => Ok(MntOverride::File(FileOverride {
            source: file,
            source_path: source,
            target: open_target_file(&target)?,
            target_path: target,
        })),
        Ok(OV::Dir(dir)) => Ok(MntOverride::Dir(DirOverride {
            source: dir,
            source_path: source,
            target: open_target_dir(&target)?,
            target_path: target,
            overlayfs: Default::default(),
        })),
        Err(e) => Err(e),
    }
}
