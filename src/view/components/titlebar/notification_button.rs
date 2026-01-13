//! Notification bell button component.

use gpui::{
    App, Entity, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, Window, div, px,
};

use crate::view::components::icons::notification_bell_icon;

struct NotificationButtonState {
    is_hovered: bool,
}

pub fn notification_button(window: &mut Window, cx: &mut App) -> impl IntoElement {
    let state: Entity<NotificationButtonState> = window.use_state(cx, |_window, _cx| {
        NotificationButtonState { is_hovered: false }
    });
    let is_hovered = state.read(cx).is_hovered;

    div()
        .id("notification-btn")
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
        .child(notification_bell_icon(is_hovered))
}
