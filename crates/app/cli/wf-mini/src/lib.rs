//! wf-mini library: the lightweight native-terminal session split into
//! reusable modules. Other crates (and the `wf-cli-demo` examples) depend on
//! these real implementations instead of keeping mirrored copies.

pub mod approval;
pub mod display;
pub mod editor;
pub mod input;
pub mod output;
pub mod render;
pub mod session;
pub mod suggest;
pub mod transcript;
