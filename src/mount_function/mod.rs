mod syscall;
pub mod types;

// `mount_setattr(dir_fd, path, flags, mount_attr)`
//
// # References
//  - [Linux]
//
// [Linux]: https://man7.org/linux/man-pages/man2/mount_setattr.2.html
//
#[inline]
pub fn mount_setattr<Path: types::Arg>(
    dir_fd: std::os::fd::BorrowedFd<'_>,
    path: Path,
    flags: types::AtFlags,
    mount_attr: &types::MountAttr<'_>,
) -> std::io::Result<()> {
    path.into_with_c_str(|c_path| syscall::mount_setattr(dir_fd, c_path, flags, mount_attr))
}
