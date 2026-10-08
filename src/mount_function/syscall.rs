use crate::mount_function::types::{AtFlags, MountAttr};
use std::ffi::CStr;
use std::io;
use std::os::fd::BorrowedFd;

#[repr(C)]
#[allow(non_camel_case_types)]
struct mount_attr {
    attr_set: u64,
    attr_clr: u64,
    propagation: u64,
    userns_fd: u64,
}

pub fn mount_setattr(
    dir_fd: BorrowedFd<'_>,
    path: &CStr,
    flags: AtFlags,
    mount_attr: &MountAttr<'_>,
) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let mut attr = mount_attr {
        attr_set: u64::from(mount_attr.attr_set.bits()),
        attr_clr: u64::from(mount_attr.attr_clr.bits()),
        propagation: u64::from(mount_attr.propagation.bits()),
        userns_fd: mount_attr
            .userns_fd
            .as_ref()
            .map_or(-nix::libc::EBADFD, AsRawFd::as_raw_fd) as u64,
    };

    let res = unsafe {
        nix::libc::syscall(
            nix::libc::c_long::from(linux_raw_sys::general::__NR_mount_setattr),
            dir_fd.as_raw_fd(),
            path.as_ptr(),
            flags.bits(),
            &mut attr,
            std::mem::size_of::<mount_attr>(),
        )
    };

    if res == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
