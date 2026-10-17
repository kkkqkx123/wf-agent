//! Named UI extension points and their render contract.

use ratatui::layout::Rect;
use ratatui::Frame;
use tui_style::theme::Theme;

/// A named UI region plugins may contribute content to.
///
/// Slots are string-named rather than an enum so a new region needs no change
/// here; the associated constants cover the regions the shell exposes, and a
/// plugin may pass [`Slot::new`] for its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot(&'static str);

impl Slot {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    pub const fn name(self) -> &'static str {
        self.0
    }

    /// Session side panel. Contributions stack top to bottom by `order`.
    pub const SIDEBAR: Slot = Slot::new("sidebar");

    /// Welcome-screen regions.
    pub const HOME_TIPS: Slot = Slot::new("home.tips");
    pub const HOME_FOOTER: Slot = Slot::new("home.footer");

    /// Transient status line.
    pub const NOTICE: Slot = Slot::new("notice");
}

/// Read-only context handed to slot content each frame. `data` carries the
/// per-frame payload a slot renders (session snapshot and the like), so slot
/// content stays `'static` while still reading live state.
pub struct SlotContext<'a, D> {
    pub theme: &'a Theme,
    pub width: u16,
    pub height: u16,
    pub data: &'a D,
}

impl<'a, D> SlotContext<'a, D> {
    pub fn new(theme: &'a Theme, width: u16, height: u16, data: &'a D) -> Self {
        Self {
            theme,
            width,
            height,
            data,
        }
    }
}

/// Content a plugin paints into a slot.
///
/// Mirrors [`crate::renderable::Renderable`] but carries theme and the
/// per-frame payload through [`SlotContext`], which the plain `Renderable`
/// contract does not. Returning `0` from [`SlotContent::desired_height`]
/// hides the content for that frame.
pub trait SlotContent<D>: Send + Sync {
    /// Rows this content needs at `width` for `data`.
    fn desired_height(&self, width: u16, data: &D) -> u16;

    /// Paint into `area`.
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &SlotContext<'_, D>);
}

/// Adapter turning a height callback and a render callback into
/// [`SlotContent`], for content that needs no dedicated type.
pub struct FnSlot<H, R> {
    height: H,
    render: R,
}

impl<H, R> FnSlot<H, R> {
    pub fn new(height: H, render: R) -> Self {
        Self { height, render }
    }
}

impl<D, H, R> SlotContent<D> for FnSlot<H, R>
where
    H: Fn(u16, &D) -> u16 + Send + Sync,
    R: Fn(&mut Frame, Rect, &SlotContext<'_, D>) + Send + Sync,
{
    fn desired_height(&self, width: u16, data: &D) -> u16 {
        (self.height)(width, data)
    }

    fn render(&self, frame: &mut Frame, area: Rect, ctx: &SlotContext<'_, D>) {
        (self.render)(frame, area, ctx)
    }
}
