//! Search bar component.

use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled, Window, div, hsla, px,
    rgb,
};

use crate::view::components::icons::PlaceholderIcon;

/// Search bar component.
#[derive(IntoElement)]
pub(super) struct SearchBar;

impl SearchBar {
    pub(super) fn new() -> Self {
        Self
    }
}

impl RenderOnce for SearchBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id("search-bar")
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(48.0))
            .w(px(475.0))
            .px(px(12.0))
            .bg(rgb(0x242424))
            .rounded(px(24.0))
            .border_1()
            .border_color(rgb(0x3e3e3e))
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(0x535353)))
            // Search icon
            .child(
                PlaceholderIcon::new()
                    .size(16.0)
                    .color(hsla(0.0, 0.0, 0.7, 1.0)),
            )
            // Placeholder text
            .child(
                div()
                    .flex_1()
                    .text_color(rgb(0xb3b3b3))
                    .text_sm()
                    .child("What do you want to play?"),
            )
            // Browse icon
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .pl(px(12.0))
                    .border_l_1()
                    .border_color(rgb(0x3e3e3e))
                    .child(
                        PlaceholderIcon::new()
                            .size(16.0)
                            .color(hsla(0.0, 0.0, 0.7, 1.0)),
                    ),
            )
    }
}
