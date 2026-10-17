//! Plugin registry, lifecycle and slot dispatch.

use std::collections::HashMap;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;
use tui_style::theme::Theme;

use super::api::{DisposeCallbacks, PluginApi};
use super::slot::{Slot, SlotContent, SlotContext};
use super::Plugin;

/// Stable reference to a plugin registered with the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PluginHandle(usize);

struct PluginEntry<D> {
    plugin: Box<dyn Plugin<D>>,
    active: bool,
}

struct SlotEntry<D> {
    plugin: String,
    order: i32,
    sequence: u64,
    content: Box<dyn SlotContent<D>>,
}

/// Owns every registered plugin and the slot content they contribute.
///
/// Registration happens at startup; [`PluginHost::activate`] /
/// [`PluginHost::deactivate`] toggle a plugin at runtime. The single dynamic
/// boundary is here: heterogeneous plugins and their content are boxed, while
/// every caller sees concrete types.
pub struct PluginHost<D> {
    entries: Vec<PluginEntry<D>>,
    ids: HashMap<String, PluginHandle>,
    slots: HashMap<Slot, Vec<SlotEntry<D>>>,
    disposes: HashMap<String, DisposeCallbacks>,
    sequence: u64,
}

impl<D: 'static> Default for PluginHost<D> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            ids: HashMap::new(),
            slots: HashMap::new(),
            disposes: HashMap::new(),
            sequence: 0,
        }
    }
}

impl<D: 'static> PluginHost<D> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `plugin` without activating it.
    ///
    /// Panics on a duplicate id: two plugins sharing an id would make
    /// deactivation ambiguous, which is a programming error rather than a
    /// runtime condition.
    pub fn register(&mut self, plugin: impl Plugin<D>) -> PluginHandle {
        let id = plugin.id().to_string();
        assert!(!self.ids.contains_key(&id), "duplicate plugin id: {id}");
        let handle = PluginHandle(self.entries.len());
        self.ids.insert(id, handle);
        self.entries.push(PluginEntry {
            plugin: Box::new(plugin),
            active: false,
        });
        handle
    }

    /// Handle for a registered plugin id.
    pub fn handle(&self, id: &str) -> Option<PluginHandle> {
        self.ids.get(id).copied()
    }

    /// Id behind `handle`.
    pub fn plugin_id(&self, handle: PluginHandle) -> &str {
        self.entries[handle.0].plugin.id()
    }

    /// Whether `handle` is currently active.
    pub fn is_active(&self, handle: PluginHandle) -> bool {
        self.entries[handle.0].active
    }

    /// Activate `handle`, merging the contributions it registers.
    pub fn activate(&mut self, handle: PluginHandle) {
        if self.entries[handle.0].active {
            return;
        }
        let id = self.entries[handle.0].plugin.id().to_string();
        let mut api = PluginApi::new();
        self.entries[handle.0].plugin.activate(&mut api);
        let (slots, disposes) = api.into_parts();
        for registration in slots {
            let sequence = self.sequence;
            self.sequence += 1;
            let entries = self.slots.entry(registration.slot).or_default();
            entries.push(SlotEntry {
                plugin: id.clone(),
                order: registration.order,
                sequence,
                content: registration.content,
            });
            entries.sort_by_key(|entry| (entry.order, entry.sequence));
        }
        if !disposes.is_empty() {
            self.disposes.entry(id).or_default().extend(disposes);
        }
        self.entries[handle.0].active = true;
    }

    /// Deactivate `handle`, removing its slot content and running its dispose
    /// callbacks in reverse registration order.
    pub fn deactivate(&mut self, handle: PluginHandle) {
        if !self.entries[handle.0].active {
            return;
        }
        let id = self.entries[handle.0].plugin.id().to_string();
        if let Some(mut stack) = self.disposes.remove(&id) {
            while let Some(dispose) = stack.pop() {
                dispose();
            }
        }
        for entries in self.slots.values_mut() {
            entries.retain(|entry| entry.plugin != id);
        }
        self.slots.retain(|_, entries| !entries.is_empty());
        self.entries[handle.0].plugin.deactivate();
        self.entries[handle.0].active = false;
    }

    /// Activate every registered plugin in registration order.
    pub fn activate_all(&mut self) {
        for index in 0..self.entries.len() {
            self.activate(PluginHandle(index));
        }
    }

    /// Deactivate every active plugin in reverse registration order.
    pub fn dispose_all(&mut self) {
        for index in (0..self.entries.len()).rev() {
            self.deactivate(PluginHandle(index));
        }
    }

    /// Whether `slot` currently has any content.
    pub fn has_slot(&self, slot: Slot) -> bool {
        self.slots
            .get(&slot)
            .is_some_and(|entries| !entries.is_empty())
    }

    /// Render every contribution in `slot`, stacked top to bottom by each
    /// contribution's declared height. Contributions that report `0` for the
    /// current width and `data` are skipped.
    pub fn render_slot(&self, slot: Slot, frame: &mut Frame, area: Rect, theme: &Theme, data: &D) {
        let Some(entries) = self.slots.get(&slot) else {
            return;
        };
        if entries.is_empty() || area.width == 0 || area.height == 0 {
            return;
        }
        let constraints: Vec<Constraint> = entries
            .iter()
            .map(|entry| Constraint::Length(entry.content.desired_height(area.width, data)))
            .collect();
        let areas = Layout::vertical(constraints).split(area);
        let ctx = SlotContext::new(theme, area.width, area.height, data);
        for (entry, sub) in entries.iter().zip(areas.iter()) {
            if sub.height == 0 {
                continue;
            }
            entry.content.render(frame, *sub, &ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::style::Style;
    use ratatui::Terminal;
    use std::sync::{Arc, Mutex};

    use crate::plugin::slot::FnSlot;

    /// Writes its marker character on the first row of the assigned area.
    struct Marker(char);

    impl SlotContent<()> for Marker {
        fn desired_height(&self, _width: u16, _data: &()) -> u16 {
            1
        }

        fn render(&self, frame: &mut Frame, area: Rect, _ctx: &SlotContext<'_, ()>) {
            frame
                .buffer_mut()
                .set_string(area.x, area.y, self.0.to_string(), Style::default());
        }
    }

    struct TestPlugin {
        id: &'static str,
        slot: Slot,
        order: i32,
        marker: char,
        log: Arc<Mutex<Vec<String>>>,
    }

    impl Plugin<()> for TestPlugin {
        fn id(&self) -> &str {
            self.id
        }

        fn activate(&mut self, api: &mut PluginApi<()>) {
            let log = Arc::clone(&self.log);
            let id = self.id.to_string();
            api.register_slot(self.slot, self.order, Marker(self.marker));
            api.on_dispose(move || log.lock().expect("log lock").push(format!("dispose:{id}")));
        }

        fn deactivate(&mut self) {
            self.log
                .lock()
                .expect("log lock")
                .push(format!("deactivate:{}", self.id));
        }
    }

    fn draw(host: &PluginHost<()>, slot: Slot, theme: &Theme) -> TestBackend {
        let backend = TestBackend::new(8, 4);
        let mut terminal = Terminal::new(backend).expect("backend");
        terminal
            .draw(|frame| host.render_slot(slot, frame, frame.area(), theme, &()))
            .expect("draw");
        terminal.backend().clone()
    }

    #[test]
    fn activation_orders_slot_content() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut host = PluginHost::new();
        let b = host.register(TestPlugin {
            id: "b",
            slot: Slot::SIDEBAR,
            order: 200,
            marker: 'B',
            log: Arc::clone(&log),
        });
        let a = host.register(TestPlugin {
            id: "a",
            slot: Slot::SIDEBAR,
            order: 100,
            marker: 'A',
            log: Arc::clone(&log),
        });
        host.activate(b);
        host.activate(a);
        assert!(host.has_slot(Slot::SIDEBAR));
        let backend = draw(&host, Slot::SIDEBAR, &Theme::dark_default());
        // Lower order paints first (top row).
        assert_eq!(backend.buffer()[(0, 0)].symbol(), "A");
        assert_eq!(backend.buffer()[(0, 1)].symbol(), "B");
    }

    #[test]
    fn deactivation_removes_content_and_disposes_in_reverse() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut host = PluginHost::new();
        let handle = host.register(TestPlugin {
            id: "mcp",
            slot: Slot::SIDEBAR,
            order: 0,
            marker: 'M',
            log: Arc::clone(&log),
        });
        host.activate(handle);
        assert!(host.has_slot(Slot::SIDEBAR));
        host.deactivate(handle);
        assert!(!host.has_slot(Slot::SIDEBAR));
        assert!(!host.is_active(handle));
        assert_eq!(
            log.lock().expect("log lock").as_slice(),
            &["dispose:mcp".to_string(), "deactivate:mcp".to_string()]
        );
    }

    #[test]
    fn dispose_all_deactivates_in_reverse_registration_order() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut host = PluginHost::new();
        for (id, marker) in [("first", '1'), ("second", '2')] {
            host.register(TestPlugin {
                id,
                slot: Slot::SIDEBAR,
                order: 0,
                marker,
                log: Arc::clone(&log),
            });
        }
        host.activate_all();
        host.dispose_all();
        let log = log.lock().expect("log lock");
        // Deactivation and disposal run in reverse: `second` before `first`.
        assert_eq!(
            log.as_slice(),
            &[
                "dispose:second".to_string(),
                "deactivate:second".to_string(),
                "dispose:first".to_string(),
                "deactivate:first".to_string(),
            ]
        );
    }

    #[test]
    fn activating_twice_registers_content_once() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut host = PluginHost::new();
        let handle = host.register(TestPlugin {
            id: "once",
            slot: Slot::SIDEBAR,
            order: 0,
            marker: 'T',
            log: Arc::clone(&log),
        });
        host.activate(handle);
        host.activate(handle);
        let backend = draw(&host, Slot::SIDEBAR, &Theme::dark_default());
        assert_eq!(backend.buffer()[(0, 0)].symbol(), "T");
        // Only one row carries the marker; the second row is untouched.
        assert_eq!(backend.buffer()[(0, 1)].symbol(), " ");
    }

    #[test]
    fn zero_height_content_is_skipped() {
        struct Hidden;
        impl SlotContent<()> for Hidden {
            fn desired_height(&self, _width: u16, _data: &()) -> u16 {
                0
            }
            fn render(&self, frame: &mut Frame, area: Rect, _ctx: &SlotContext<'_, ()>) {
                frame
                    .buffer_mut()
                    .set_string(area.x, area.y, "X", Style::default());
            }
        }

        struct HidePlugin;
        impl Plugin<()> for HidePlugin {
            fn id(&self) -> &str {
                "hide"
            }
            fn activate(&mut self, api: &mut PluginApi<()>) {
                api.register_slot(Slot::SIDEBAR, 0, Hidden);
            }
            fn deactivate(&mut self) {}
        }

        let mut host = PluginHost::new();
        let handle = host.register(HidePlugin);
        host.activate(handle);
        let backend = draw(&host, Slot::SIDEBAR, &Theme::dark_default());
        assert_eq!(backend.buffer()[(0, 0)].symbol(), " ");
    }

    #[test]
    #[should_panic(expected = "duplicate plugin id: dup")]
    fn duplicate_id_panics() {
        struct Noop;
        impl Plugin<()> for Noop {
            fn id(&self) -> &str {
                "dup"
            }
            fn activate(&mut self, _api: &mut PluginApi<()>) {}
            fn deactivate(&mut self) {}
        }
        let mut host = PluginHost::new();
        host.register(Noop);
        host.register(Noop);
    }

    #[test]
    fn fn_slot_reports_and_renders() {
        let content = FnSlot::new(
            |_width: u16, _data: &()| 1u16,
            |frame: &mut Frame, area: Rect, _ctx: &SlotContext<'_, ()>| {
                frame
                    .buffer_mut()
                    .set_string(area.x, area.y, "Z", Style::default());
            },
        );
        assert_eq!(content.desired_height(80, &()), 1);
        let theme = Theme::dark_default();
        let backend = TestBackend::new(4, 1);
        let mut terminal = Terminal::new(backend).expect("backend");
        terminal
            .draw(|frame| {
                let area = frame.area();
                content.render(frame, area, &SlotContext::new(&theme, 4, 1, &()));
            })
            .expect("draw");
        assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), "Z");
    }
}
