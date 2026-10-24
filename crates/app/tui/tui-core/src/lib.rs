//! tui-core: low-level TUI implementation crate.

pub mod anchor;
pub mod copy;
pub mod event_dispatch;
pub mod events;
pub mod frame_metrics;
pub mod framer;
pub mod headless;
pub mod keymap;
pub mod observability;
pub mod panel_select;
pub mod perf;
pub mod plugin;
pub mod prep_keys;
pub mod redraw;
pub mod reducer;
pub mod render_model;
pub mod renderable;
pub mod screen_data;
pub mod status_line;
pub mod stream_pacer;
pub mod tail;
pub mod tool_output;
pub mod visual_capture;
pub mod width;
pub mod wrap_map;
