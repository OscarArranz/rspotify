//! Friends/users button component.

use gpui::{
    App, Entity, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::view::components::icons::friends_outlined_icon;

struct FriendsButtonState {
    is_hovered: bool,
}

pub fn friends_button(window: &mut Window, cx: &mut App) -> impl IntoElement {
    let state: Entity<FriendsButtonState> =
        window.use_state(cx, |_window, _cx| FriendsButtonState { is_hovered: false });
    let is_hovered = state.read(cx).is_hovered;

    div()
        .id("friends-btn")
        .flex()
        .items_center()
        .justify_center()
        .size(px(32.0))
        .rounded_full()
        .cursor_pointer()
        .on_hover(move |hovered, _window, cx| {
            let state = state.clone();
            state.update(cx, |state, cx| {
                state.is_hovered = *hovered;
                cx.notify();
            });
        })
        .child(friends_outlined_icon(is_hovered))
}
