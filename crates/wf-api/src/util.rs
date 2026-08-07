//! Crate-internal numeric helpers shared across modules.

/// Round a float to two decimal places.
pub(crate) fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
