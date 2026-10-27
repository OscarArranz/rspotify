//! Shared Liked Songs status visuals. The player bar and playlist rows use the same
//! plus/check language so account membership reads consistently across the app.

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb, svg};

pub(crate) const GREEN: u32 = 0x1ed760;
pub(crate) const MUTED: u32 = 0xb3b3b3;

/// Filled green circle with a white check for a track that is in Liked Songs.
pub(crate) fn liked_check(size: f32) -> AnyElement {
    div()
        .size(px(size))
        .rounded_full()
        .bg(rgb(GREEN))
        .flex()
        .items_center()
        .justify_center()
        .child(
            svg()
                .path("icons/check.svg")
                .size(px(size * 0.58))
                .text_color(rgb(0xffffff)),
        )
        .into_any_element()
}

/// Outlined circle with a plus for a track that can be saved to Liked Songs.
pub(crate) fn like_plus(size: f32, color: u32) -> AnyElement {
    div()
        .size(px(size))
        .rounded_full()
        .border_1()
        .border_color(rgb(color))
        .flex()
        .items_center()
        .justify_center()
        .child(
            svg()
                .path("icons/plus.svg")
                .size(px(size * 0.56))
                .text_color(rgb(color)),
        )
        .into_any_element()
}
