# fs_overlay — Design Document

## Intro

`fs_overlay` is a prototype wrapper for running FHS-based programs on NixOS without using Bubblewrap. Bubblewrap restricts privilege escalation, preventing workflows such as using `sudo` in FHS shells, running `pkexec`, or writing files as root from editors like VS Code.

The goal of `fs_overlay` is to use mount namespaces only, preserving the ability to escalate privileges while safely overlaying parts of `/` (such as `/bin`, `/lib`, etc.) for a specific program.

You can use bind mounts or overlayfs. Overlayfs is only supported in kernels newer or equal to 6.13 because before then you could not use file descriptors to configure it using the new mount api.

The config files should be configured with the same care as sudo configuration files. `fs_overlay` should also be compiled with musl to have no dynamic dependencies.


## High-Level Approach

* Use mount namespaces without user namespaces.
* Overlay or bind-mount directories and files as specified in a configuration file.
* Replace selected paths under `/` with controlled alternatives.
* After overrides set, execute the target program as the invoking user.



## Execution Model

Invocation format:

```
fs_overlay config_for_program.toml program [args...]
```

Execution steps:

1. Look for config file in `/etc/fs_overlay/` and validate configuration file integrity and permissions.
2. Create a new mount namespace.
3. Apply directory and file overrides.
4. Drop privileges to the user and group who executed the `fs_overlay`
5. Execute the target program.



## Global Constraints
* All filesystem and mount operations must be performed through file descriptors obtained that have been verified to have correct permissions, ownership and etc.
* All mounts are readonly and `MOUNT_ATTR_NOSUID` is set. File bind mounts are mounted with `MOUNT_ATTR_NOSYMFOLLOW`.
* All paths must be absolute and not contain any `..`.

## Privilege and Permission Model

### fs_overlay Binary

* Must run as root.
* Intended to be setuid root.
* Behavior is driven entirely by configuration files.

### Configuration Files
* Must be owned by root:root.
* Must not be writable others.
* Parent directories must be owned by root:root.
* No special permission bits are allowed on the ancestors.
* Must be readable by others.



### file overrides
#### Sources
* Source files must be owned by root:root and not writable by others.
* Source files must not be symlinks.
* Ancestors must not have special permissions bits set except sticky bit and must be owned by root:anygroup.
* Direct parent must not have any special permissions bits set and must be owned by root:root.

File source must be from under these paths
- `/nix/store/store-object/`

#### Targets
* Target files must be owned by root:root.
* Ancestors must not have any special permissions bits set and must be owned by root:root.

File targets must be from under these paths
- Any path under `/lib64/`
- Any path under `/lib32/`
- Any path under `/lib/`
- Any path under `/libexec/`
- Any path under `/bin/`
- Any path under `/sbin/`

### directory overrides
If the target directory does not exist, it is created with permissions 755 and ownership root:root. A bind mount can be used or Overlayfs. With only lower dirs set and no upper dir. So no whiteouts is possible.

#### sources
* All ancestor directories must be owned by root:anygroup and have no special permission bits except sticky bit.
* No special permission bits are allowed on the parent directory and must be owned by root:root.
* Subdirectories in the source directory must not be symlinks.
* Symlinks in the source directories must point to a file that follow the same rules as sources in file overrides.
* All contained files must follow the same rules as sources in file overrides.


#### targets
* All ancestor directories must be owned by root:root and have no special permission bits.
* Target directory must be root:root.

Allowed paths to override
- Any path under and including `/lib64/`
- Any path under and including `/lib32/`
- Any path under and including `/lib/`
- Any path under and including `/libexec/`
- Any path under and including `/bin/`
- Any path under and including `/sbin/`


## Example config
```toml
[file_overrides]
"/bin/bash" = "/nix/store/x29azqwvawz9f6q57pin2srsn2ax8w08-example-bash/bash"

[dir_overrides]
"/lib64" = { source = "/nix/store/x29azqwvawz9f6q57pin2srsn2ax8w08-example-libs/lib", overlayfs = true }
```