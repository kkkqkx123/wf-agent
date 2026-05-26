use crate::{sandbox::Sandbox, syscall::translate_path, vfs::mount::MountTable};
use reverie::{syscalls::Syscall, Error, Guest};

/// The `llistxattr` system call.
///
/// This intercepts llistxattr syscalls and translates paths according to the mount table.
/// Returns Some(syscall) if the path was translated and should be injected,
/// or None if the original syscall should be used.
pub async fn handle_llistxattr<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Llistxattr,
    mount_table: &MountTable,
) -> Result<Option<Syscall>, Error> {
    if let Some(path_addr) = args.path() {
        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = args.with_path(Some(new_path_addr));

            return Ok(Some(Syscall::Llistxattr(new_syscall)));
        }
    }
    Ok(None)
}

/// The `lgetxattr` system call.
///
/// This intercepts lgetxattr syscalls and translates paths according to the mount table.
/// Returns Some(syscall) if the path was translated and should be injected,
/// or None if the original syscall should be used.
pub async fn handle_lgetxattr<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Lgetxattr,
    mount_table: &MountTable,
) -> Result<Option<Syscall>, Error> {
    if let Some(path_addr) = args.path() {
        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = args.with_path(Some(new_path_addr));

            return Ok(Some(Syscall::Lgetxattr(new_syscall)));
        }
    }
    Ok(None)
}
