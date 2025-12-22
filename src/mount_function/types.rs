use bitflags::bitflags;

bitflags! {
    /// `MOUNT_ATTR_*` constants for use with [`fsmount`, `mount_setattr`].
    ///
    /// [`fsmount`]: crate::mount::fsmount
    /// [`mount_setattr`]: crate::mount::mount_setattr
    #[repr(transparent)]
    #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
    pub struct MountAttrFlags: u32 {
        /// `MOUNT_ATTR_RDONLY`
        const MOUNT_ATTR_RDONLY = linux_raw_sys::general::MOUNT_ATTR_RDONLY;

        /// `MOUNT_ATTR_NOSUID`
        const MOUNT_ATTR_NOSUID = linux_raw_sys::general::MOUNT_ATTR_NOSUID;

        /// `MOUNT_ATTR_NODEV`
        const MOUNT_ATTR_NODEV = linux_raw_sys::general::MOUNT_ATTR_NODEV;

        /// `MOUNT_ATTR_NOEXEC`
        const MOUNT_ATTR_NOEXEC = linux_raw_sys::general::MOUNT_ATTR_NOEXEC;

        /// `MOUNT_ATTR__ATIME`
        const MOUNT_ATTR__ATIME = linux_raw_sys::general::MOUNT_ATTR__ATIME;

        /// `MOUNT_ATTR_RELATIME`
        const MOUNT_ATTR_RELATIME = linux_raw_sys::general::MOUNT_ATTR_RELATIME;

        /// `MOUNT_ATTR_NOATIME`
        const MOUNT_ATTR_NOATIME = linux_raw_sys::general::MOUNT_ATTR_NOATIME;

        /// `MOUNT_ATTR_STRICTATIME`
        const MOUNT_ATTR_STRICTATIME = linux_raw_sys::general::MOUNT_ATTR_STRICTATIME;

        /// `MOUNT_ATTR_NODIRATIME`
        const MOUNT_ATTR_NODIRATIME = linux_raw_sys::general::MOUNT_ATTR_NODIRATIME;

        /// `MOUNT_ATTR_NOUSER`
        const MOUNT_ATTR_IDMAP = linux_raw_sys::general::MOUNT_ATTR_IDMAP;

        /// `MOUNT_ATTR__ATIME_FLAGS`
        const MOUNT_ATTR_NOSYMFOLLOW = linux_raw_sys::general::MOUNT_ATTR_NOSYMFOLLOW;

        /// `MOUNT_ATTR__ATIME_FLAGS`
        const MOUNT_ATTR_SIZE_VER0 = linux_raw_sys::general::MOUNT_ATTR_SIZE_VER0;

        /// <https://docs.rs/bitflags/*/bitflags/#externally-defined-flags>
        const _ = !0;
    }
}

bitflags! {
    /// `MS_*` constants for use with [`mount_change`].
    ///
    /// [`mount_change`]: crate::mount::mount_change
    #[repr(transparent)]
    #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
    pub struct MountPropagationFlags: u32 {
        /// `MS_SILENT`
        const SILENT = linux_raw_sys::general::MS_SILENT;
        /// `MS_SHARED`
        const SHARED = linux_raw_sys::general::MS_SHARED;
        /// `MS_PRIVATE`
        const PRIVATE = linux_raw_sys::general::MS_PRIVATE;
        /// `MS_SLAVE`
        const SLAVE = linux_raw_sys::general::MS_SLAVE;
        /// `MS_UNBINDABLE`
        const UNBINDABLE = linux_raw_sys::general::MS_UNBINDABLE;
        /// `MS_REC`
        const REC = linux_raw_sys::general::MS_REC;

        /// <https://docs.rs/bitflags/*/bitflags/#externally-defined-flags>
        const _ = !0;
    }
}

bitflags! {
    /// `AT_*` constants for use with [`openat`], [`statat`], and other `*at`
    /// functions.
    ///
    /// [`openat`]: crate::fs::openat
    /// [`statat`]: crate::fs::statat
    #[repr(transparent)]
    #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
    pub struct AtFlags: u32 {
        /// `AT_SYMLINK_NOFOLLOW`
        const SYMLINK_NOFOLLOW = linux_raw_sys::general::AT_SYMLINK_NOFOLLOW;

        /// `AT_EACCESS`
        const EACCESS = linux_raw_sys::general::AT_EACCESS;

        /// `AT_REMOVEDIR`
        const REMOVEDIR = linux_raw_sys::general::AT_REMOVEDIR;

        /// `AT_SYMLINK_FOLLOW`
        const SYMLINK_FOLLOW = linux_raw_sys::general::AT_SYMLINK_FOLLOW;

        /// `AT_NO_AUTOMOUNT`
        const NO_AUTOMOUNT = linux_raw_sys::general::AT_NO_AUTOMOUNT;

        /// `AT_EMPTY_PATH`
        const EMPTY_PATH = linux_raw_sys::general::AT_EMPTY_PATH;

        /// `AT_RECURSIVE`
        const RECURSIVE = linux_raw_sys::general::AT_RECURSIVE;

        /// `AT_STATX_SYNC_AS_STAT`
        const STATX_SYNC_AS_STAT = linux_raw_sys::general::AT_STATX_SYNC_AS_STAT;

        /// `AT_STATX_FORCE_SYNC`
        const STATX_FORCE_SYNC = linux_raw_sys::general::AT_STATX_FORCE_SYNC;

        /// `AT_STATX_DONT_SYNC`
        const STATX_DONT_SYNC = linux_raw_sys::general::AT_STATX_DONT_SYNC;

        /// <https://docs.rs/bitflags/*/bitflags/#externally-defined-flags>
        const _ = !0;
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
#[allow(missing_docs)]
pub struct MountAttr<'a> {
    pub attr_set: MountAttrFlags,
    pub attr_clr: MountAttrFlags,
    pub propagation: MountPropagationFlags,
    pub userns_fd: Option<std::os::fd::BorrowedFd<'a>>,
}

pub trait Arg {
    /// Returns a view of this string as a string slice.
    #[allow(unused)]
    fn as_str(&self) -> std::io::Result<&str>;

    /// Returns a potentially-lossy rendering of this string as a
    /// `Cow<'_, str>`.
    #[allow(unused)]
    fn to_string_lossy(&self) -> std::borrow::Cow<'_, str>;

    /// Returns a view of this string as a maybe-owned [`CStr`].
    #[allow(unused)]
    fn as_cow_c_str(&self) -> std::io::Result<std::borrow::Cow<'_, core::ffi::CStr>>;

    /// Consumes `self` and returns a view of this string as a maybe-owned
    /// [`CStr`].
    #[allow(unused)]
    fn into_c_str<'b>(self) -> std::io::Result<std::borrow::Cow<'b, core::ffi::CStr>>
    where
        Self: 'b;

    /// Runs a closure with `self` passed in as a `&CStr`.
    fn into_with_c_str<T, F>(self, f: F) -> std::io::Result<T>
    where
        Self: Sized,
        F: FnOnce(&core::ffi::c_str::CStr) -> std::io::Result<T>;
}

impl Arg for &str {
    #[inline]
    fn as_str(&self) -> std::io::Result<&str> {
        Ok(self)
    }

    #[inline]
    fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Borrowed(self)
    }

    #[inline]
    fn as_cow_c_str(&self) -> std::io::Result<std::borrow::Cow<'_, core::ffi::CStr>> {
        Ok(std::borrow::Cow::Owned(
            std::ffi::CString::new(*self)
                .map_err(|_| std::io::Error::from_raw_os_error(nix::libc::EINVAL))?,
        ))
    }

    #[inline]
    fn into_c_str<'b>(self) -> std::io::Result<std::borrow::Cow<'b, core::ffi::CStr>>
    where
        Self: 'b,
    {
        Ok(std::borrow::Cow::Owned(
            std::ffi::CString::new(self)
                .map_err(|_| std::io::Error::from_raw_os_error(nix::libc::EINVAL))?,
        ))
    }

    #[inline]
    fn into_with_c_str<T, F>(self, f: F) -> std::io::Result<T>
    where
        Self: Sized,
        F: FnOnce(&core::ffi::CStr) -> std::io::Result<T>,
    {
        let c_str = std::ffi::CString::new(self)
            .map_err(|_| std::io::Error::from_raw_os_error(nix::libc::EINVAL))?;
        f(&c_str)
    }
}
