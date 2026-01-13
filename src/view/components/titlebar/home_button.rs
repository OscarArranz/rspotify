//! Home button component.

use gpui::{
    App, Entity, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement, Styled, Window, div, px, rgb,
};

use crate::view::components::icons::{PlaceholderIcon, home_icon};

use super::BUTTON_HOVER_BG;

struct HomeButtonState {
    is_hovered: bool,
    is_active: bool,
}

pub fn home_button(window: &mut Window, cx: &mut App) -> impl IntoElement {
    let state: Entity<HomeButtonState> = window.use_state(cx, |_window, _cx| HomeButtonState {
        is_hovered: false,
        is_active: false,
    });
    let is_hovered = state.read(cx).is_hovered;
    let is_active = state.read(cx).is_active;

    div()
        .id("home-btn")
        .flex()
        .items_center()
        .justify_center()
        .size(px(48.0))
        .rounded_full()
        .bg(rgb(0x1f1f1f))
        .cursor_pointer()
        .on_hover(move |hovered, _window, cx| {
            let state = state.clone();
            state.update(cx, |state, cx| {
                state.is_hovered = *hovered;
                cx.notify();
            });
        })
        .hover(|style| style.bg(rgb(BUTTON_HOVER_BG)))
        .child(home_icon(is_hovered, is_active))
}
