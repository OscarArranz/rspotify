//! Root layout component.
//!
//! This is the top-level layout that contains:
//! - The custom titlebar (always visible)
//! - The path router for main content

use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb,
};

use crate::router::{PathRouter, PathRouterHandle, Route};
use crate::view::components::titlebar::Titlebar;
use crate::view::main_layout::MainLayout;
use crate::view::screens::sign_in::SignInScreen;

/// Root layout containing titlebar and main content area.
pub struct RootLayout {
    router: Entity<PathRouter>,
    titlebar: Entity<Titlebar>,
}

impl RootLayout {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Create the path router with routes
        let router = cx.new(|_| {
            PathRouter::new()
                .route(Route::new("/sign-in", |_window, cx| {
                    cx.new(|_| SignInScreen::new()).into()
                }))
                .route(Route::new("/main", |window, cx| {
                    cx.new(|cx| MainLayout::new(window, cx)).into()
                }))
                .initial_path("/sign-in")
        });

        // Register path router globally so components can navigate
        let router_handle = PathRouterHandle::new(router.clone());
        cx.set_global(router_handle);

        Self {
            router,
            titlebar: cx.new(|cx| Titlebar::new(cx)),
        }
    }
}

impl Render for RootLayout {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .font_family("Spotify Mix UI Title Var Wide")
            .bg(rgb(0x121212))
            // Titlebar at the top
            .child(self.titlebar.clone())
            // Main content area below titlebar
            .child(div().flex_1().overflow_hidden().child(self.router.clone()))
    }
}
