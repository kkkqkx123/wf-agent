//! Process exit codes shared by every wf CLI frontend.
//!
//! Values follow `sysexits.h` wherever a matching condition exists. Two
//! deliberate deviations are documented here so they stay intentional:
//!   * `INTERRUPTED` uses the POSIX shell convention `128 + SIGINT` (130);
//!     sysexits.h defines no interruption code.
//!   * `DATAERR` also carries generic business failures because sysexits.h
//!     defines no dedicated business-failure code.
//!
//! This module is the single source of truth. Frontends and tools must use
//! these constants instead of bare numbers so code semantics cannot drift.

/// Successful termination (`EX_OK`).
pub const OK: u8 = 0;
/// Command-line usage error (`EX_USAGE`).
pub const USAGE: u8 = 64;
/// Malformed input data or unmet business expectation (`EX_DATAERR`).
pub const DATAERR: u8 = 65;
/// Required input cannot be opened or is missing (`EX_NOINPUT`).
pub const NOINPUT: u8 = 66;
/// Addressed entity does not exist (`EX_NOUSER`).
pub const NOUSER: u8 = 67;
/// Service unreachable — local relay and remote are equivalent (`EX_UNAVAILABLE`).
pub const UNAVAILABLE: u8 = 69;
/// Internal software fault (`EX_SOFTWARE`).
pub const SOFTWARE: u8 = 70;
/// Input/output failure (`EX_IOERR`).
pub const IOERR: u8 = 74;
/// Transient failure, caller is invited to retry (`EX_TEMPFAIL`).
pub const TEMPFAIL: u8 = 75;
/// Remote protocol or response error (`EX_PROTOCOL`).
pub const PROTOCOL: u8 = 76;
/// Permission denied (`EX_NOPERM`).
pub const NOPERM: u8 = 77;
/// Configuration error (`EX_CONFIG`).
pub const CONFIG: u8 = 78;
/// Interrupted by SIGINT (`128 + 2`); POSIX shell convention, not sysexits.
pub const INTERRUPTED: u8 = 130;

/// Map a stable remote business code to a sysexits-aligned exit code.
///
/// A local relay and a remote service are treated as equivalent: the code
/// describes the business outcome, not where the server happens to run.
pub fn remote_exit_code(code: &str) -> u8 {
    match code {
        "SESSION_BUSY" => TEMPFAIL,
        "SESSION_NOT_FOUND" => NOUSER,
        "LOCATION_NOT_FOUND" => NOINPUT,
        "FORBIDDEN" | "TICKET_INVALID" => NOPERM,
        "SERVICE_UNAVAILABLE" => UNAVAILABLE,
        _ => PROTOCOL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysexits_values_are_stable() {
        assert_eq!(OK, 0);
        assert_eq!(USAGE, 64);
        assert_eq!(DATAERR, 65);
        assert_eq!(NOINPUT, 66);
        assert_eq!(NOUSER, 67);
        assert_eq!(UNAVAILABLE, 69);
        assert_eq!(SOFTWARE, 70);
        assert_eq!(IOERR, 74);
        assert_eq!(TEMPFAIL, 75);
        assert_eq!(PROTOCOL, 76);
        assert_eq!(NOPERM, 77);
        assert_eq!(CONFIG, 78);
        assert_eq!(INTERRUPTED, 130);
    }

    #[test]
    fn remote_business_codes_are_distinguished() {
        assert_eq!(remote_exit_code("SESSION_BUSY"), TEMPFAIL);
        assert_eq!(remote_exit_code("SESSION_NOT_FOUND"), NOUSER);
        assert_eq!(remote_exit_code("LOCATION_NOT_FOUND"), NOINPUT);
        assert_eq!(remote_exit_code("FORBIDDEN"), NOPERM);
        assert_eq!(remote_exit_code("TICKET_INVALID"), NOPERM);
        assert_eq!(remote_exit_code("SERVICE_UNAVAILABLE"), UNAVAILABLE);
        assert_eq!(remote_exit_code("SOMETHING_NEW"), PROTOCOL);
    }
}
