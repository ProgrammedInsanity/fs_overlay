use std::os::fd::AsFd;

#[derive(thiserror::Error, Debug)]
pub enum OverlayError {
    #[error("Failed to open overlay fs")]
    FsOpen(#[source] std::io::Error),
    #[error("Failed to set redirect_dir")]
    SetRedirectDir(#[source] std::io::Error),
    #[error("Failed to set metacopy")]
    SetMetacopy(#[source] std::io::Error),
    #[error("Failed to add lowerdir source")]
    AddLowerDirSource(#[source] std::io::Error),
    #[error("Failed to add lowerdir target")]
    AddLowerDirTarget(#[source] std::io::Error),
    #[error("Failed to create overlay config")]
    FsConfigCreate(#[source] std::io::Error),
    #[error("Failed to mount overlay")]
    FsMount(#[source] std::io::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to open tree for {0:?}")]
    OpenTree(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to set mount attributes for {0:?}")]
    SetAttr(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to move mount from {0:?} to {1:?}")]
    MoveMount(
        std::path::PathBuf,
        std::path::PathBuf,
        #[source] std::io::Error,
    ),
    #[error("Validation failed: {0}")]
    Validation(#[from] super::utils::Error),
    #[error("overlayfs is not supported for kernel versions < 6.13")]
    OverlayFSNotSupported,
    #[error("OverlayFS operation failed: {0}")]
    Overlay(#[from] OverlayError),
    #[error("Kernel version parse failed for '{0}': {1}")]
    KernelVersionParse(String, #[source] semver::Error),
}

pub fn unshare_and_privatise_mounts() {
    use crate::mount_function::mount_setattr;
    use crate::mount_function::types::{AtFlags, MountAttr, MountAttrFlags, MountPropagationFlags};
    use crate::utils::open_root;
    use rustix::thread::UnshareFlags;
    use rustix::thread::unshare_unsafe;

    unsafe { unshare_unsafe(UnshareFlags::NEWNS).expect("Could not unshare from mount namespace") }

    mount_setattr(
        open_root().expect("Failed to open /").as_fd(),
        "",
        AtFlags::EMPTY_PATH | AtFlags::RECURSIVE,
        &MountAttr {
            attr_clr: MountAttrFlags::empty(),
            attr_set: MountAttrFlags::empty(),
            propagation: MountPropagationFlags::PRIVATE,
            userns_fd: None,
        },
    )
    .expect("Failed to make / private");
}

pub fn mount(config: super::config::Input) -> Result<(), Error> {
    // I never read a man page fully before this, I got to say that it is pretty usefull.
    // I hate to mix rustix and nix but I wrote a big deal with nix and why change what works...
    // The new mount api was very fun to work with, Everyone definitely switch to it. 

    for file_override in &config.file_overrides {
        set_file_bind_mount(file_override)?;
        if cfg!(debug_assertions) {
            println!(
                "Bind mounted {} over {}",
                file_override.source_path.display(),
                file_override.target_path.display()
            );
        }
    }

    for dir_override in &config.dir_overrides {
        if dir_override.overlayfs {
            set_dir_overlayfs(dir_override)?;

            if cfg!(debug_assertions) {
                println!(
                    "Set overlayfs {} over {}",
                    dir_override.source_path.display(),
                    dir_override.target_path.display()
                );
            }
        } else {
            set_dir_bind_mount(dir_override)?;
            if cfg!(debug_assertions) {
                println!(
                    "Bind mounted {} over {}",
                    dir_override.source_path.display(),
                    dir_override.target_path.display()
                );
            }
        }
    }

    Ok(())
}

fn set_file_bind_mount(file_override: &super::config::FileOverride) -> Result<(), Error> {
    use crate::mount_function::mount_setattr;
    use crate::mount_function::types::{AtFlags, MountAttr, MountAttrFlags, MountPropagationFlags};
    use rustix::mount::{MoveMountFlags, OpenTreeFlags, move_mount, open_tree};

    let detached_src_fd = open_tree(
        &file_override.source,
        "",
        OpenTreeFlags::AT_EMPTY_PATH | OpenTreeFlags::OPEN_TREE_CLONE,
    )
    .map_err(|e| Error::OpenTree(file_override.source_path.clone(), e.into()))?;

    mount_setattr(
        detached_src_fd.as_fd(),
        "",
        AtFlags::EMPTY_PATH,
        &MountAttr {
            attr_clr: MountAttrFlags::empty(),
            attr_set: MountAttrFlags::MOUNT_ATTR_NOSYMFOLLOW
                | MountAttrFlags::MOUNT_ATTR_RDONLY
                | MountAttrFlags::MOUNT_ATTR_NOSUID,
            propagation: MountPropagationFlags::PRIVATE,
            userns_fd: None,
        },
    )
    .map_err(|e| Error::SetAttr(file_override.source_path.clone(), e))?;

    move_mount(
        detached_src_fd,
        "",
        &file_override.target,
        "",
        MoveMountFlags::MOVE_MOUNT_F_EMPTY_PATH | MoveMountFlags::MOVE_MOUNT_T_EMPTY_PATH,
    )
    .map_err(|e| {
        Error::MoveMount(
            file_override.source_path.clone(),
            file_override.target_path.clone(),
            e.into(),
        )
    })?;

    Ok(())
}

fn set_dir_bind_mount(dir_override: &super::config::DirOverride) -> Result<(), Error> {
    use crate::mount_function::mount_setattr;
    use crate::mount_function::types::{AtFlags, MountAttr, MountAttrFlags, MountPropagationFlags};
    use rustix::mount::{MoveMountFlags, OpenTreeFlags, move_mount, open_tree};

    let detached_src_fd = open_tree(
        &dir_override.source,
        "",
        OpenTreeFlags::AT_EMPTY_PATH | OpenTreeFlags::OPEN_TREE_CLONE,
    )
    .map_err(|e| Error::OpenTree(dir_override.source_path.clone(), e.into()))?;

    mount_setattr(
        detached_src_fd.as_fd(),
        "",
        AtFlags::EMPTY_PATH,
        &MountAttr {
            attr_clr: MountAttrFlags::empty(),
            attr_set: MountAttrFlags::MOUNT_ATTR_RDONLY | MountAttrFlags::MOUNT_ATTR_NOSUID,
            propagation: MountPropagationFlags::PRIVATE,
            userns_fd: None,
        },
    )
    .map_err(|e| Error::SetAttr(dir_override.source_path.clone(), e))?;

    move_mount(
        detached_src_fd,
        "",
        &dir_override.target,
        "",
        MoveMountFlags::MOVE_MOUNT_F_EMPTY_PATH | MoveMountFlags::MOVE_MOUNT_T_EMPTY_PATH,
    )
    .map_err(|e| {
        Error::MoveMount(
            dir_override.source_path.clone(),
            dir_override.target_path.clone(),
            e.into(),
        )
    })?;

    Ok(())
}

fn set_dir_overlayfs(dir_override: &super::config::DirOverride) -> Result<(), Error> {
    use crate::mount_function::mount_setattr;
    use crate::mount_function::types::{AtFlags, MountAttr, MountAttrFlags, MountPropagationFlags};
    use rustix::mount::{
        FsMountFlags, FsOpenFlags, MoveMountFlags, fsconfig_create, fsconfig_set_fd,
        fsconfig_set_string, fsmount, fsopen, move_mount,
    };
    {
        use rustix::system::uname;
        use semver::Version;

        let uname_release = uname().release().to_string_lossy().into_owned();
        let kernel_version = Version::parse(&uname_release)
            .map_err(|e| Error::KernelVersionParse(uname_release.clone(), e))?;
        let earliest_supported_kernel = Version::parse("6.13.0").unwrap();
        if kernel_version < earliest_supported_kernel {
            return Err(Error::OverlayFSNotSupported);
        }
    }

    let overlayfs =
        fsopen("overlay", FsOpenFlags::empty()).map_err(|e| OverlayError::FsOpen(e.into()))?;

    fsconfig_set_string(&overlayfs, "redirect_dir", "nofollow")
        .map_err(|e| OverlayError::SetRedirectDir(e.into()))?;
    fsconfig_set_string(&overlayfs, "metacopy", "off")
        .map_err(|e| OverlayError::SetMetacopy(e.into()))?;

    fsconfig_set_fd(&overlayfs, "lowerdir+", &dir_override.source)
        .map_err(|e| OverlayError::AddLowerDirSource(e.into()))?;
    fsconfig_set_fd(&overlayfs, "lowerdir+", &dir_override.target)
        .map_err(|e| OverlayError::AddLowerDirTarget(e.into()))?;

    fsconfig_create(&overlayfs).map_err(|e| OverlayError::FsConfigCreate(e.into()))?;

    let overlayfs = fsmount(
        overlayfs,
        FsMountFlags::empty(),
        rustix::mount::MountAttrFlags::empty(),
    )
    .map_err(|e| OverlayError::FsMount(e.into()))?;

    mount_setattr(
        overlayfs.as_fd(),
        "",
        AtFlags::EMPTY_PATH,
        &MountAttr {
            attr_clr: MountAttrFlags::empty(),
            attr_set: MountAttrFlags::MOUNT_ATTR_RDONLY | MountAttrFlags::MOUNT_ATTR_NOSUID,
            propagation: MountPropagationFlags::PRIVATE,
            userns_fd: None,
        },
    )
    .map_err(|e| Error::SetAttr(dir_override.source_path.clone(), e))?;

    move_mount(
        overlayfs,
        "",
        &dir_override.target,
        "",
        MoveMountFlags::MOVE_MOUNT_F_EMPTY_PATH | MoveMountFlags::MOVE_MOUNT_T_EMPTY_PATH,
    )
    .map_err(|e| {
        Error::MoveMount(
            dir_override.source_path.clone(),
            dir_override.target_path.clone(),
            e.into(),
        )
    })?;

    Ok(())
}
