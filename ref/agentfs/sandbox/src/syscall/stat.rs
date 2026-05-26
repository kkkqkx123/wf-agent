use crate::{
    sandbox::Sandbox,
    syscall::translate_path,
    vfs::{fdtable::FdTable, mount::MountTable},
};
use reverie::{
    syscalls::{MemoryAccess, ReadAddr, Syscall},
    Error, Guest, Stack,
};

/// The `statx` system call.
///
/// This intercepts `statx` system calls and translates paths according to the mount table
/// and virtualizes the dirfd.
/// Returns `Some(result)` if the syscall was handled and the result should be returned directly,
/// or `None` if the original syscall should be used.
pub async fn handle_statx<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Statx,
    mount_table: &MountTable,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let dirfd = args.dirfd();
    // AT_FDCWD is -100
    let kernel_dirfd = if dirfd == -100 {
        dirfd
    } else {
        fd_table.translate(dirfd).unwrap_or(dirfd)
    };

    if let Some(path_addr) = args.path() {
        // Read the original path from guest memory
        let path: std::path::PathBuf = path_addr.read(&guest.memory())?;

        // Check if this path matches a mount point
        if let Some((vfs, _translated_path)) = mount_table.resolve(&path) {
            // Check if this is a virtual VFS (like SQLite)
            if vfs.is_virtual() {
                // For virtual VFS, statx is not supported - return ENOSYS
                // The caller will fall back to newfstatat
                return Ok(Some(-libc::ENOSYS as i64));
            }
        }

        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Statx::new()
                .with_dirfd(kernel_dirfd)
                .with_path(Some(new_path_addr))
                .with_flags(args.flags())
                .with_mask(args.mask())
                .with_statx(args.statx());

            let result = guest.inject(Syscall::Statx(new_syscall)).await?;
            return Ok(Some(result));
        }
    }
    Ok(None)
}

/// The `newfstatat` system call.
///
/// This intercepts `newfstatat` system calls and translates paths according to the mount table
/// and virtualizes the dirfd.
/// Returns `Some(result)` if the syscall was handled and the result should be returned directly,
/// or `None` if the original syscall should be used.
#[cfg(not(target_arch = "aarch64"))]
pub async fn handle_newfstatat<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Newfstatat,
    mount_table: &MountTable,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    use reverie::syscalls::AtFlags;
    let dirfd = args.dirfd();
    // AT_FDCWD is -100
    let kernel_dirfd = if dirfd == -100 {
        dirfd
    } else {
        fd_table.translate(dirfd).unwrap_or(dirfd)
    };

    if let Some(path_addr) = args.path() {
        // Read the original path from guest memory
        let path: std::path::PathBuf = path_addr.read(&guest.memory())?;

        // Check if this path matches a mount point
        if let Some((vfs, _translated_path)) = mount_table.resolve(&path) {
            // Check if this is a virtual VFS (like SQLite)
            if vfs.is_virtual() {
                let flags = args.flags();
                let follow_symlinks = !flags.contains(AtFlags::AT_SYMLINK_NOFOLLOW);

                let stat_result = if follow_symlinks {
                    vfs.stat(&path).await
                } else {
                    vfs.lstat(&path).await
                };

                match stat_result {
                    Ok(stat_buf) => {
                        // Write the stat result to guest memory
                        if let Some(stat_addr) = args.stat() {
                            // Convert stat struct to bytes and write
                            let stat_bytes: &[u8] = unsafe {
                                std::slice::from_raw_parts(
                                    &stat_buf as *const _ as *const u8,
                                    std::mem::size_of::<libc::stat>(),
                                )
                            };
                            guest
                                .memory()
                                .write_exact(stat_addr.0.cast::<u8>(), stat_bytes)?;
                        }
                        return Ok(Some(0)); // Success
                    }
                    Err(e) => {
                        // Map VFS errors to errno
                        let errno = match e {
                            crate::vfs::VfsError::NotFound => -libc::ENOENT as i64,
                            crate::vfs::VfsError::PermissionDenied => -libc::EACCES as i64,
                            _ => -libc::EIO as i64,
                        };
                        return Ok(Some(errno));
                    }
                }
            }
        }

        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Newfstatat::new()
                .with_dirfd(kernel_dirfd)
                .with_path(Some(new_path_addr))
                .with_stat(args.stat())
                .with_flags(args.flags());

            let result = guest.inject(Syscall::Newfstatat(new_syscall)).await?;
            return Ok(Some(result));
        }
    }
    Ok(None)
}

/// The `statfs` system call.
///
/// This intercepts `statfs` system calls and translates paths according to the mount table.
pub async fn handle_statfs<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Statfs,
    mount_table: &MountTable,
) -> Result<Option<Syscall>, Error> {
    if let Some(path_addr) = args.path() {
        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = args.with_path(Some(new_path_addr));

            return Ok(Some(Syscall::Statfs(new_syscall)));
        }
    }
    Ok(None)
}

/// The `readlink` system call.
///
/// This intercepts `readlink` system calls and translates paths according to the mount table.
#[cfg(not(target_arch = "aarch64"))]
pub async fn handle_readlink<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Readlink,
    mount_table: &MountTable,
) -> Result<Option<i64>, Error> {
    if let Some(path_addr) = args.path() {
        let path: std::path::PathBuf = path_addr.read(&guest.memory())?;

        // Check if this path matches a mount point
        if let Some((vfs, _translated_path)) = mount_table.resolve(&path) {
            // Check if this is a virtual VFS (like SQLite)
            if vfs.is_virtual() {
                // Call VFS readlink method directly
                match vfs.readlink(&path).await {
                    Ok(target) => {
                        // Write the target to the user's buffer
                        if let Some(buf_addr) = args.buf() {
                            let bufsize = args.bufsize();
                            let target_str = target.to_string_lossy();
                            let target_bytes = target_str.as_bytes();
                            let bytes_to_write = std::cmp::min(target_bytes.len(), bufsize);

                            guest.memory().write_exact(
                                buf_addr.cast::<u8>(),
                                &target_bytes[..bytes_to_write],
                            )?;

                            return Ok(Some(bytes_to_write as i64));
                        }
                        return Ok(Some(0));
                    }
                    Err(e) => {
                        // Map VFS errors to errno
                        let errno = match e {
                            crate::vfs::VfsError::NotFound => -libc::ENOENT as i64,
                            crate::vfs::VfsError::PermissionDenied => -libc::EACCES as i64,
                            _ => -libc::EINVAL as i64,
                        };
                        return Ok(Some(errno));
                    }
                }
            }
        }

        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Readlink::new()
                .with_path(Some(new_path_addr))
                .with_buf(args.buf())
                .with_bufsize(args.bufsize());

            let result = guest.inject(Syscall::Readlink(new_syscall)).await?;
            return Ok(Some(result));
        }
    }
    Ok(None)
}

/// The `readlinkat` system call.
///
/// This intercepts `readlinkat` system calls and translates paths according to the mount table
/// and virtualizes the dirfd.
pub async fn handle_readlinkat<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Readlinkat,
    mount_table: &MountTable,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let dirfd = args.dirfd();
    // AT_FDCWD is -100
    let kernel_dirfd = if dirfd == -100 {
        dirfd
    } else {
        fd_table.translate(dirfd).unwrap_or(dirfd)
    };

    if let Some(path_addr) = args.path() {
        let path: std::path::PathBuf = path_addr.read(&guest.memory())?;

        // Check if this path matches a mount point
        if let Some((vfs, _translated_path)) = mount_table.resolve(&path) {
            // Check if this is a virtual VFS (like SQLite)
            if vfs.is_virtual() {
                // Call VFS readlink method directly
                match vfs.readlink(&path).await {
                    Ok(target) => {
                        // Write the target to the user's buffer
                        if let Some(buf_addr) = args.buf() {
                            let bufsize = args.buf_len();
                            let target_str = target.to_string_lossy();
                            let target_bytes = target_str.as_bytes();
                            let bytes_to_write = std::cmp::min(target_bytes.len(), bufsize);

                            guest.memory().write_exact(
                                buf_addr.cast::<u8>(),
                                &target_bytes[..bytes_to_write],
                            )?;

                            return Ok(Some(bytes_to_write as i64));
                        }
                        return Ok(Some(0));
                    }
                    Err(e) => {
                        // Map VFS errors to errno
                        let errno = match e {
                            crate::vfs::VfsError::NotFound => -libc::ENOENT as i64,
                            crate::vfs::VfsError::PermissionDenied => -libc::EACCES as i64,
                            _ => -libc::EINVAL as i64,
                        };
                        return Ok(Some(errno));
                    }
                }
            }
        }

        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Readlinkat::new()
                .with_dirfd(kernel_dirfd)
                .with_path(Some(new_path_addr))
                .with_buf(args.buf())
                .with_buf_len(args.buf_len());

            let result = guest.inject(Syscall::Readlinkat(new_syscall)).await?;
            return Ok(Some(result));
        }
    }
    Ok(None)
}

/// The `symlink` system call.
///
/// This intercepts `symlink` system calls and translates the linkpath according to the mount table.
/// The target path is left as-is since it's just a string stored in the symlink.
/// Returns `Some(result)` if the syscall was handled and the result should be returned directly,
/// or `None` if the original syscall should be used.
#[cfg(not(target_arch = "aarch64"))]
pub async fn handle_symlink<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Symlink,
    mount_table: &MountTable,
) -> Result<Option<i64>, Error> {
    // Read the linkpath from guest memory
    if let Some(linkpath_addr) = args.linkpath() {
        let linkpath: std::path::PathBuf = linkpath_addr.read(&guest.memory())?;

        // Read the target from guest memory
        if let Some(target_addr) = args.target() {
            let target: std::path::PathBuf = target_addr.read(&guest.memory())?;

            // Check if this path matches a mount point
            if let Some((vfs, _translated_path)) = mount_table.resolve(&linkpath) {
                // Check if this is a virtual VFS (like SQLite)
                if vfs.is_virtual() {
                    // Call VFS symlink method directly
                    match vfs.symlink(&target, &linkpath).await {
                        Ok(()) => return Ok(Some(0)), // Success
                        Err(e) => {
                            // Map VFS errors to errno
                            let errno = match e {
                                crate::vfs::VfsError::NotFound => -libc::ENOENT as i64,
                                crate::vfs::VfsError::PermissionDenied => -libc::EACCES as i64,
                                crate::vfs::VfsError::AlreadyExists => -libc::EEXIST as i64,
                                _ => -libc::EIO as i64,
                            };
                            return Ok(Some(errno));
                        }
                    }
                }
            }

            if let Some(new_linkpath_addr) =
                translate_path(guest, linkpath_addr, mount_table).await?
            {
                let new_syscall = reverie::syscalls::Symlink::new()
                    .with_target(args.target())
                    .with_linkpath(Some(new_linkpath_addr));

                let result = guest.inject(Syscall::Symlink(new_syscall)).await?;
                return Ok(Some(result));
            }
        }
    }
    Ok(None)
}

/// The `symlinkat` system call.
///
/// This intercepts `symlinkat` system calls and translates the linkpath according to the mount table
/// and virtualizes the dirfd.
/// The target path is left as-is since it's just a string stored in the symlink.
/// Returns `Some(result)` if the syscall was handled and the result should be returned directly,
/// or `None` if the original syscall should be used.
pub async fn handle_symlinkat<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Symlinkat,
    mount_table: &MountTable,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let dirfd = args.newdirfd();
    // AT_FDCWD is -100
    let kernel_dirfd = if dirfd == -100 {
        dirfd
    } else {
        fd_table.translate(dirfd).unwrap_or(dirfd)
    };

    // Read linkpath and target from guest memory
    if let Some(linkpath_addr) = args.linkpath() {
        let linkpath: std::path::PathBuf = linkpath_addr.read(&guest.memory())?;

        if let Some(target_addr) = args.target() {
            let target: std::path::PathBuf = target_addr.read(&guest.memory())?;

            // Check if this path matches a mount point
            if let Some((vfs, _translated_path)) = mount_table.resolve(&linkpath) {
                // Check if this is a virtual VFS (like SQLite)
                if vfs.is_virtual() {
                    // Call VFS symlink method directly
                    match vfs.symlink(&target, &linkpath).await {
                        Ok(()) => return Ok(Some(0)), // Success
                        Err(e) => {
                            // Map VFS errors to errno
                            let errno = match e {
                                crate::vfs::VfsError::NotFound => -libc::ENOENT as i64,
                                crate::vfs::VfsError::PermissionDenied => -libc::EACCES as i64,
                                crate::vfs::VfsError::AlreadyExists => -libc::EEXIST as i64,
                                _ => -libc::EIO as i64,
                            };
                            return Ok(Some(errno));
                        }
                    }
                }
            }

            if let Some(new_linkpath_addr) =
                translate_path(guest, linkpath_addr, mount_table).await?
            {
                let new_syscall = reverie::syscalls::Symlinkat::new()
                    .with_target(args.target())
                    .with_newdirfd(kernel_dirfd)
                    .with_linkpath(Some(new_linkpath_addr));

                let result = guest.inject(Syscall::Symlinkat(new_syscall)).await?;
                return Ok(Some(result));
            }
        }
    }
    Ok(None)
}

/// The `linkat` system call.
///
/// This intercepts `linkat` system calls and translates paths according to the mount table
/// and virtualizes the dirfds.
/// Returns `Some(result)` if the syscall was handled and the result should be returned directly,
/// or `None` if the original syscall should be used.
pub async fn handle_linkat<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Linkat,
    mount_table: &MountTable,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let olddirfd = args.olddirfd();
    let newdirfd = args.newdirfd();

    // AT_FDCWD is -100
    let kernel_olddirfd = if olddirfd == -100 {
        olddirfd
    } else {
        fd_table.translate(olddirfd).unwrap_or(olddirfd)
    };
    let kernel_newdirfd = if newdirfd == -100 {
        newdirfd
    } else {
        fd_table.translate(newdirfd).unwrap_or(newdirfd)
    };

    // Read oldpath and newpath from guest memory
    if let Some(oldpath_addr) = args.oldpath() {
        let oldpath: std::path::PathBuf = oldpath_addr.read(&guest.memory())?;

        if let Some(newpath_addr) = args.newpath() {
            let newpath: std::path::PathBuf = newpath_addr.read(&guest.memory())?;

            // Check if newpath matches a mount point with virtual VFS
            if let Some((vfs, _translated_path)) = mount_table.resolve(&newpath) {
                // Check if this is a virtual VFS (like SQLite)
                if vfs.is_virtual() {
                    // Call VFS link method directly
                    match vfs.link(&oldpath, &newpath).await {
                        Ok(()) => return Ok(Some(0)), // Success
                        Err(e) => {
                            // Map VFS errors to errno
                            let errno = match e {
                                crate::vfs::VfsError::NotFound => -libc::ENOENT as i64,
                                crate::vfs::VfsError::PermissionDenied => -libc::EPERM as i64,
                                crate::vfs::VfsError::AlreadyExists => -libc::EEXIST as i64,
                                _ => -libc::EIO as i64,
                            };
                            return Ok(Some(errno));
                        }
                    }
                }
            }

            // Check if either path needs translation by consulting the mount table.
            // We resolve both paths first (without guest memory allocation) to determine
            // what needs translation, then allocate and inject if needed.
            let oldpath_translated = mount_table.resolve(&oldpath);
            let newpath_translated = mount_table.resolve(&newpath);

            match (oldpath_translated, newpath_translated) {
                (Some((_vfs1, translated_oldpath)), Some((_vfs2, translated_newpath))) => {
                    // Both paths need translation
                    use std::ffi::CString;

                    let old_cstr = CString::new(translated_oldpath.to_string_lossy().to_string())
                        .map_err(|_| reverie::syscalls::Errno::EINVAL)?;
                    let new_cstr = CString::new(translated_newpath.to_string_lossy().to_string())
                        .map_err(|_| reverie::syscalls::Errno::EINVAL)?;

                    // Allocate space for both paths on guest stack
                    let old_bytes = old_cstr.as_bytes_with_nul();
                    let new_bytes = new_cstr.as_bytes_with_nul();

                    let mut stack = guest.stack().await;
                    let old_addr: reverie::syscalls::AddrMut<std::path::PathBuf> = stack.reserve();
                    let new_addr: reverie::syscalls::AddrMut<std::path::PathBuf> = stack.reserve();
                    stack.commit()?;

                    let old_byte_addr = old_addr.cast::<u8>();
                    let new_byte_addr = new_addr.cast::<u8>();
                    guest.memory().write_exact(old_byte_addr, old_bytes)?;
                    guest.memory().write_exact(new_byte_addr, new_bytes)?;

                    let new_oldpath_ptr: reverie::syscalls::PathPtr =
                        unsafe { std::mem::transmute(old_byte_addr) };
                    let new_newpath_ptr: reverie::syscalls::PathPtr =
                        unsafe { std::mem::transmute(new_byte_addr) };

                    let new_syscall = reverie::syscalls::Linkat::new()
                        .with_olddirfd(kernel_olddirfd)
                        .with_oldpath(Some(new_oldpath_ptr))
                        .with_newdirfd(kernel_newdirfd)
                        .with_newpath(Some(new_newpath_ptr))
                        .with_flags(args.flags());
                    let result = guest.inject(Syscall::Linkat(new_syscall)).await?;
                    return Ok(Some(result));
                }
                (Some(_), None) => {
                    // Only oldpath needs translation
                    if let Some(new_oldpath_addr) =
                        translate_path(guest, oldpath_addr, mount_table).await?
                    {
                        let new_syscall = reverie::syscalls::Linkat::new()
                            .with_olddirfd(kernel_olddirfd)
                            .with_oldpath(Some(new_oldpath_addr))
                            .with_newdirfd(kernel_newdirfd)
                            .with_newpath(Some(newpath_addr))
                            .with_flags(args.flags());
                        let result = guest.inject(Syscall::Linkat(new_syscall)).await?;
                        return Ok(Some(result));
                    }
                }
                (None, Some(_)) => {
                    // Only newpath needs translation
                    if let Some(new_newpath_addr) =
                        translate_path(guest, newpath_addr, mount_table).await?
                    {
                        let new_syscall = reverie::syscalls::Linkat::new()
                            .with_olddirfd(kernel_olddirfd)
                            .with_oldpath(Some(oldpath_addr))
                            .with_newdirfd(kernel_newdirfd)
                            .with_newpath(Some(new_newpath_addr))
                            .with_flags(args.flags());
                        let result = guest.inject(Syscall::Linkat(new_syscall)).await?;
                        return Ok(Some(result));
                    }
                }
                (None, None) => {
                    // Neither path needs translation
                }
            }
        }
    }
    Ok(None)
}
