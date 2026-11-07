//! Back and Forward navigation buttons component.

use gpui::{
    App, Div, InteractiveElement, IntoElement, MouseButton, ParentElement, Stateful, Styled,
    Window, div, prelude::FluentBuilder, px,
};

use crate::{
    hooks::use_stack_nav,
    view::components::icons::{back_icon, forward_icon},
};

pub fn navigation_buttons(_window: &mut Window, cx: &mut App) -> impl IntoElement {
    let stack_nav = use_stack_nav(cx);
    let nav_for_click = stack_nav.clone();

    match stack_nav {
        Some(nav) => {
            let nav = nav.clone();
            let can_go_back = nav.can_go_back(cx);
            let can_go_forward = nav.can_go_forward(cx);

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
                        .child(div().child(back_icon(can_go_back))),
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
                        .child(div().child(forward_icon(can_go_forward))),
                )
        }
        None => div(),
    }
}
