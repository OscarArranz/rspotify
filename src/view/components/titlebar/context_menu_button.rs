//! Three-dot context menu button component.

use gpui::{
    App, InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, Styled, Window,
    div, px, rgb,
};

use super::BUTTON_HOVER_BG;

/// Three-dot context menu button.
#[derive(IntoElement)]
pub(super) struct ContextMenuButton;

impl ContextMenuButton {
    pub(super) fn new() -> Self {
        Self
    }
}

impl RenderOnce for ContextMenuButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id("context-menu-btn")
            .flex()
            .items_center()
            .justify_center()
            .size(px(32.0))
            .ml(px(18.0))
            .rounded(px(4.0))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                // Three horizontal dots
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .child(div().size(px(4.0)).rounded_full().bg(gpui::white()))
                    .child(div().size(px(4.0)).rounded_full().bg(gpui::white()))
                    .child(div().size(px(4.0)).rounded_full().bg(gpui::white())),
            )
    }
}
