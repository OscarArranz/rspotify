//! Back and Forward navigation buttons component.

use gpui::{
    App, Div, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce,
    Stateful, Styled, Window, div, hsla, prelude::FluentBuilder, px, rgb,
};

use crate::{
    hooks::use_stack_nav,
    view::components::icons::{back_icon, forward_icon},
};

use super::BUTTON_HOVER_BG;

struct NavigationButtonsState {
    back_button_is_hovered: bool,
    forward_button_is_hovered: bool,
}

pub fn navigation_buttons(window: &mut Window, cx: &mut App) -> impl IntoElement {
    let stack_nav = use_stack_nav(cx);
    let nav_for_click = stack_nav.clone();

    match stack_nav {
        Some(nav) => {
            let nav = nav.clone();
            let can_go_back = nav.can_go_back(cx);
            let can_go_forward = nav.can_go_forward(cx);
            let state: Entity<NavigationButtonsState> =
                window.use_state(cx, |_window, _cx| NavigationButtonsState {
                    back_button_is_hovered: false,
                    forward_button_is_hovered: false,
                });
            let back_button_is_hovered = state.read(cx).back_button_is_hovered;
            let forward_button_is_hovered = state.read(cx).forward_button_is_hovered;

            div()
                .flex()
                .items_center()
                .ml(px(8.0))
                .gap(px(2.0))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                // Back button
                .child(
                    div()
                        .id("back-btn")
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(32.0))
                        .when(can_go_back, |this: Stateful<Div>| {
                            let nav = nav_for_click.clone();
                            this.cursor_pointer()
                                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                                    if let Some(ref nav) = nav {
                                        nav.go_back(cx);
                                    }
                                })
                        })
                        .child(div().child(back_icon(back_button_is_hovered))),
                )
                .child(
                    div()
                        .id("forward-btn")
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(32.0))
                        .when(can_go_forward, |this: Stateful<Div>| {
                            let nav = nav_for_click.clone();
                            this.cursor_pointer()
                                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                                    if let Some(ref nav) = nav {
                                        nav.go_forward(cx);
                                    }
                                })
                        })
                        .child(div().child(forward_icon(forward_button_is_hovered))),
                )
        }
        None => div(),
    }
}
