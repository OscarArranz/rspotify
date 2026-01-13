//! Custom window titlebar component.
//!
//! Implements a Spotify-style titlebar with:
//! - Context menu button (left)
//! - Back/Forward navigation buttons (when authenticated)
//! - Home button (when authenticated)
//! - Search bar (when authenticated)
//! - Notification and friends buttons (when authenticated)
//! - User avatar (when authenticated)
//! - Window control buttons (minimize, maximize, close)

mod context_menu_button;
mod friends_button;
mod home_button;
mod navigation_buttons;
mod notification_button;
mod search_bar;
mod user_avatar;
mod window_controls;

use gpui::{
    App, Div, InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, Stateful,
    Styled, Window, div, prelude::FluentBuilder, px, rgb,
};
use gpui::{AppContext, Context, Entity, Render, WindowControlArea};

use crate::hooks::is_authenticated;
use crate::view::components::titlebar::friends_button::friends_button;
use crate::view::components::titlebar::home_button::home_button;
use crate::view::components::titlebar::navigation_buttons::navigation_buttons;
use crate::view::components::titlebar::notification_button::notification_button;

use context_menu_button::ContextMenuButton;
use search_bar::SearchBar;
use user_avatar::UserAvatar;
use window_controls::WindowControls;

/// The height of the titlebar in pixels.
pub const TITLEBAR_HEIGHT: f32 = 64.0;

/// Background color for the titlebar.
const TITLEBAR_BG: u32 = 0x000000;

/// Button hover background color.
const BUTTON_HOVER_BG: u32 = 0x282828;

/// Close button hover background (red).
const CLOSE_BUTTON_HOVER_BG: u32 = 0xe81123;

/// Disabled button color.
const DISABLED_COLOR: u32 = 0x4d4d4d;

// We need this piece of code to handle the dragging of the titlebar
struct TitleBarState {
    should_move: bool,
}

// TODO: Remove this when GPUI has released v0.2.3
impl Render for TitleBarState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Custom Spotify-style titlebar component.
pub struct Titlebar {
    user_avatar: Entity<UserAvatar>,
}

impl Titlebar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            user_avatar: cx.new(|_| UserAvatar::new()),
        }
    }
}

impl Render for Titlebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let authenticated = is_authenticated(cx);
        let state = window.use_state(cx, |_, _| TitleBarState { should_move: false });

        div()
            .id("titlebar")
            .window_control_area(WindowControlArea::Drag)
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(TITLEBAR_HEIGHT))
            .bg(rgb(TITLEBAR_BG))
            // .px(px(8.0))
            // Make the titlebar draggable for window movement
            .map(|this| {
                this.on_mouse_down_out(window.listener_for(&state, |state, _, _window, _| {
                    state.should_move = false;
                }))
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    window.listener_for(&state, |state, _, _, _| {
                        state.should_move = false;
                    }),
                )
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    window.listener_for(&state, |state, _, _, _| {
                        state.should_move = true;
                    }),
                )
                .on_mouse_move(window.listener_for(
                    &state,
                    |state, _, window, _| {
                        if state.should_move {
                            state.should_move = false;
                            window.start_window_move();
                        }
                    },
                ))
            })
            .child(
                // Left section: Menu button and navigation
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(ContextMenuButton::new())
                    .when(authenticated, |this: Div| {
                        this.child(navigation_buttons(window, cx))
                    }),
            )
            .when(authenticated, |this: Stateful<Div>| {
                this.child(
                    // Center section: Home button and search bar
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(home_button(window, cx))
                        .child(SearchBar::new()),
                )
            })
            .child(
                // Right section: Notifications, friends, user, and window controls
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(14.0))
                    .when(authenticated, |this: Div| {
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .mr(px(4.0))
                                        .child(notification_button(window, cx))
                                        .child(friends_button(window, cx)),
                                )
                                .child(self.user_avatar.clone()),
                        )
                    })
                    .child(WindowControls::new()),
            )
    }
}
