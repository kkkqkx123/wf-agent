//! tui-style: theme, animation and motion for low-level TUI crates.
//!
//! Optimization isolation: [`anim_core`] holds dependency-free numeric cores
//! (cycle math, angle lookup tables, color conversion) with no clock,
//! terminal or style dependencies, so hot frame math stays testable and
//! optimizable in isolation at any optimization level. Table lookups replay
//! the same float accumulation as direct computation bitwise (see
//! `angle_table_matches_direct_computation_bitwise`). The [`animation`] and
//! [`motion`] layers are thin clock-injected adapters over that core; policy
//! decisions (tiers, environment switches) live above in `tui-core::perf`.

pub mod anim_core;
pub mod animation;
pub mod motion;
pub mod theme;
pub mod theme_mode;
