use gpui::{
    AsyncApp, Context, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Render,
    Styled, Window, div, prelude::*, px, rgb,
};

use crate::hooks::use_app_state;
use crate::router::{PathRouterHandle, StackRouter, StackRouterHandle};
use crate::state::AppState;

/// A simple home screen to show inside the stack router.
pub struct HomeScreen;

impl Render for HomeScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scopes = use_app_state(cx, |state, _| {
            state
                .auth
                .scope
                .clone()
                .unwrap_or_else(|| "User".to_string())
        });

        div()
            .flex()
            .flex_col()
            .gap_4()
            .size_full()
            .justify_center()
            .items_center()
            .child(
                div()
                    .text_2xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Welcome to Spotify!"),
            )
            .child(
                div()
                    .text_color(rgb(0xb3b3b3))
                    .child(format!("Scopes: {}", scopes)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().size_8().bg(gpui::red()))
                    .child(div().size_8().bg(rgb(0x1DB954))) // Spotify green
                    .child(div().size_8().bg(gpui::blue()))
                    .child(div().size_8().bg(gpui::yellow()))
                    .child(div().size_8().bg(gpui::black()))
                    .child(div().size_8().bg(gpui::white())),
            )
    }
}

/// Main layout containing the stack router and navigation.
///
/// This is the primary view after login, containing:
/// - A header with logout button
/// - The stack router for in-app navigation
pub struct MainLayout {
    stack_router: Entity<StackRouter>,
}

impl MainLayout {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Create the stack router with HomeScreen as initial screen
        let home = cx.new(|_| HomeScreen);
        let stack_router = cx.new(|_| StackRouter::with_initial(home.into()));

        // Register stack router globally so child components can navigate
        let stack_handle = StackRouterHandle::new(stack_router.clone());
        cx.set_global(stack_handle);

        Self { stack_router }
    }
}

impl Render for MainLayout {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let router_handle = cx.global::<PathRouterHandle>().clone();
        let window_handle = window.window_handle();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x121212)) // Spotify dark background
            .text_color(rgb(0xffffff))
            // Header with logout button
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .w_full()
                    .px(px(16.0))
                    .py(px(12.0))
                    .bg(rgb(0x000000))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0x1DB954))
                            .child("Spotify"),
                    )
                    .child(
                        div()
                            .id("logout-button")
                            .px(px(16.0))
                            .py(px(8.0))
                            .bg(rgb(0x282828))
                            .hover(|style| style.bg(rgb(0x3e3e3e)))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                                let router = router_handle.clone();
                                let window_handle = window_handle.clone();

                                // Clear auth state
                                cx.update_global::<AppState, _>(|state, _cx| {
                                    state.logout();
                                    println!("Logged out");
                                });

                                // Navigate back to sign-in
                                cx.spawn(async move |cx: &mut AsyncApp| {
                                    window_handle
                                        .update(cx, |_, window, cx| {
                                            router.navigate("/sign-in", window, cx);
                                        })
                                        .ok();
                                })
                                .detach();
                            })
                            .child("Logout"),
                    ),
            )
            // Main content area with stack router
            .child(div().flex_1().child(self.stack_router.clone()))
    }
}
