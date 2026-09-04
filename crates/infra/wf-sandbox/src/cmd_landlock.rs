//! Landlock filesystem enforcement for the command hardening gateway.
//!
//! Applies path-beneath rules in the forked child before exec so the
//! sandboxed process can only touch the directories the policy allows.

use std::io;
use std::path::{Path, PathBuf};

use wf_types::script::sandbox::PathPolicy;

const SYS_LANDLOCK_CREATE_RULESET: i64 = 444;
const SYS_LANDLOCK_ADD_RULE: i64 = 445;
const SYS_LANDLOCK_RESTRICT_SELF: i64 = 446;
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;

const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1 << 12;

/// All ABI-v1 access rights. ABI-2/3 bits (TRUNCATE, REFER) are excluded so
/// the ruleset works on every Landlock kernel.
const LANDLOCK_HANDLED_FS: u64 = LANDLOCK_ACCESS_FS_EXECUTE
    | LANDLOCK_ACCESS_FS_WRITE_FILE
    | LANDLOCK_ACCESS_FS_READ_FILE
    | LANDLOCK_ACCESS_FS_READ_DIR
    | LANDLOCK_ACCESS_FS_REMOVE_DIR
    | LANDLOCK_ACCESS_FS_REMOVE_FILE
    | LANDLOCK_ACCESS_FS_MAKE_CHAR
    | LANDLOCK_ACCESS_FS_MAKE_DIR
    | LANDLOCK_ACCESS_FS_MAKE_REG
    | LANDLOCK_ACCESS_FS_MAKE_SOCK
    | LANDLOCK_ACCESS_FS_MAKE_FIFO
    | LANDLOCK_ACCESS_FS_MAKE_BLOCK
    | LANDLOCK_ACCESS_FS_MAKE_SYM;

/// Read-only access: execute + read file + read dir.
const LANDLOCK_READ_ACCESS: u64 =
    LANDLOCK_ACCESS_FS_EXECUTE | LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR;

/// Read-write access (mirrors the seccomp model: write does NOT imply
/// remove, so REMOVE_* bits are intentionally absent).
const LANDLOCK_WRITE_ACCESS: u64 = LANDLOCK_READ_ACCESS
    | LANDLOCK_ACCESS_FS_WRITE_FILE
    | LANDLOCK_ACCESS_FS_MAKE_REG
    | LANDLOCK_ACCESS_FS_MAKE_DIR
    | LANDLOCK_ACCESS_FS_MAKE_SYM
    | LANDLOCK_ACCESS_FS_MAKE_SOCK
    | LANDLOCK_ACCESS_FS_MAKE_FIFO
    | LANDLOCK_ACCESS_FS_MAKE_CHAR
    | LANDLOCK_ACCESS_FS_MAKE_BLOCK;

#[repr(C)]
struct LandlockRulesetAttr {
    handled_access_fs: u64,
}

#[repr(C)]
struct LandlockPathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

/// Whether Landlock is usable on this kernel (checked once).
fn landlock_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let attr = LandlockRulesetAttr {
            handled_access_fs: 0,
        };
        let fd = unsafe { libc::syscall(SYS_LANDLOCK_CREATE_RULESET, &attr as *const _, 0usize) };
        if fd < 0 {
            return false;
        }
        unsafe {
            libc::close(fd as i32);
        }
        true
    })
}

/// Resolve a possibly-relative path against `cwd` so Landlock rules do not
/// depend on the child's working directory at rule-add time.
fn absolutize(path: &str, cwd: Option<&Path>) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else if let Some(cwd) = cwd {
        cwd.join(p)
    } else {
        p.to_path_buf()
    }
}

/// Apply a Landlock ruleset to the current process (must be called from the
/// single-threaded child before exec, after `PR_SET_NO_NEW_PRIVS`).
///
/// Fail-closed: an error while adding a rule aborts the spawn. Unsupported
/// kernels (ENOSYS/EOPNOTSUPP) are skipped instead — the seccomp and
/// analysis gates still apply.
pub(crate) fn apply_landlock(path_policy: &PathPolicy, cwd: Option<&Path>) -> io::Result<()> {
    if !landlock_available() {
        return Ok(());
    }

    let attr = LandlockRulesetAttr {
        handled_access_fs: LANDLOCK_HANDLED_FS,
    };
    let ruleset_fd =
        unsafe { libc::syscall(SYS_LANDLOCK_CREATE_RULESET, &attr as *const _, 0usize) };
    if ruleset_fd < 0 {
        let err = io::Error::last_os_error();
        // Unsupported kernels/configs are tolerated (seccomp still applies);
        // any other failure is fatal.
        return match err.raw_os_error() {
            Some(libc::ENOSYS) | Some(libc::EOPNOTSUPP) => Ok(()),
            _ => Err(err),
        };
    }
    let ruleset_fd = ruleset_fd as i32;

    // Each rule targets the allowed path itself (opened with O_PATH): for a
    // directory the access applies to it and everything beneath it; for a
    // file it applies to that file only. `parent_fd` of the ABI-v1 struct is
    // this object fd; the ABI-v2 `path` member is left zero-filled by the
    // kernel's copy_struct_from_user when the shorter struct is passed.
    let add_rule = |allowed_access: u64, path: &str| -> io::Result<()> {
        let abs = absolutize(path, cwd);
        let path_c = std::ffi::CString::new(abs.to_string_lossy().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid path"))?;
        let object_fd = unsafe { libc::open(path_c.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if object_fd < 0 {
            // The path does not exist (yet); the vfs-gate would have denied
            // access to it anyway. Skip instead of failing the whole command.
            return Ok(());
        }
        let beneath = LandlockPathBeneathAttr {
            allowed_access,
            parent_fd: object_fd,
        };
        let ret = unsafe {
            libc::syscall(
                SYS_LANDLOCK_ADD_RULE,
                ruleset_fd,
                LANDLOCK_RULE_PATH_BENEATH,
                &beneath as *const _,
                0usize,
            )
        };
        unsafe {
            libc::close(object_fd);
        }
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    };

    for path in &path_policy.allowed_read {
        add_rule(LANDLOCK_READ_ACCESS, path)?;
    }
    for path in &path_policy.allowed_write {
        add_rule(LANDLOCK_WRITE_ACCESS, path)?;
    }

    let ret = unsafe { libc::syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset_fd, 0usize) };
    unsafe {
        libc::close(ruleset_fd);
    }
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
