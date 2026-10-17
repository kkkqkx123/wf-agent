//! Registration buffer handed to a plugin while it activates.

use super::slot::{Slot, SlotContent};

/// Cleanup callbacks registered by one plugin, run on deactivation in reverse.
pub(crate) type DisposeCallbacks = Vec<Box<dyn FnOnce() + Send>>;

/// Accumulates the contributions of one plugin activation.
///
/// The host hands a fresh buffer to [`super::Plugin::activate`] and merges the
/// result only after activation returns, so a plugin never borrows the host
/// while it registers.
pub struct PluginApi<D> {
    slots: Vec<SlotRegistration<D>>,
    disposes: DisposeCallbacks,
}

pub(crate) struct SlotRegistration<D> {
    pub slot: Slot,
    pub order: i32,
    pub content: Box<dyn SlotContent<D>>,
}

impl<D: 'static> Default for PluginApi<D> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            disposes: Vec::new(),
        }
    }
}

impl<D: 'static> PluginApi<D> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Contribute `content` to `slot`; content within one slot renders in
    /// ascending `order`, ties broken by registration sequence.
    pub fn register_slot(
        &mut self,
        slot: Slot,
        order: i32,
        content: impl SlotContent<D> + 'static,
    ) {
        self.slots.push(SlotRegistration {
            slot,
            order,
            content: Box::new(content),
        });
    }

    /// Register a cleanup callback, run on deactivation in reverse order.
    pub fn on_dispose(&mut self, dispose: impl FnOnce() + Send + 'static) {
        self.disposes.push(Box::new(dispose));
    }

    pub(crate) fn into_parts(self) -> (Vec<SlotRegistration<D>>, DisposeCallbacks) {
        (self.slots, self.disposes)
    }
}
