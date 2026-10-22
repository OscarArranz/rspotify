//! Root layout component.
//!
//! This is the top-level layout that contains:
//! - The custom titlebar (always visible)
//! - The path router for main content

use futures::channel::oneshot;
use gpui::{
    AsyncApp, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*,
    rgb,
};

use crate::lib::spotify::NativeSpotify;
use crate::router::{PathRouter, PathRouterHandle, Route};
use crate::state::AppState;
use crate::view::components::titlebar::Titlebar;
use crate::view::main_layout::MainLayout;
use crate::view::screens::sign_in::SignInScreen;

/// Root layout containing titlebar and main content area.
pub struct RootLayout {
    router: Entity<PathRouter>,
    titlebar: Entity<Titlebar>,
}

impl RootLayout {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let restoring = NativeSpotify::has_restorable_session();
        if restoring && let Some(profile) = NativeSpotify::cached_profile() {
            cx.update_global::<AppState, _>(|state, _| state.set_profile(profile));
        }
        // Create the path router with routes
        let router = cx.new(|_| {
            PathRouter::new()
                .route(Route::new("/sign-in", |_window, cx| {
                    cx.new(|_| SignInScreen::new()).into()
                }))
                .route(Route::new("/main", |window, cx| {
                    cx.new(|cx| MainLayout::new(window, cx)).into()
                }))
                .initial_path(if restoring { "/main" } else { "/sign-in" })
        });

        // Register path router globally so components can navigate
        let router_handle = PathRouterHandle::new(router.clone());
        cx.set_global(router_handle);

        if restoring {
            let router_handle = cx.global::<PathRouterHandle>().clone();
            let window_handle = window.window_handle();
            cx.spawn(async move |_this, cx: &mut AsyncApp| {
                let (tx, rx) = oneshot::channel();
                std::thread::spawn(move || {
                    let result = NativeSpotify::restore();
                    let _ = tx.send(result);
                });
                match rx.await {
                    Ok(Ok(client)) => {
                        let profile = client
                            .persist_profile()
                            .or_else(NativeSpotify::cached_profile);
                        let _ = cx.update(|cx| {
                            cx.update_global::<AppState, _>(|state, _| {
                                state.set_spotify_client(client);
                                if let Some(profile) = profile {
                                    state.set_profile(profile);
                                }
                            });
                        });
                    }
                    Ok(Err(crate::lib::spotify::SpotifyError::InvalidRefreshToken)) => {
                        NativeSpotify::clear_saved_refresh_token();
                        let _ = cx.update(|cx| {
                            cx.update_global::<AppState, _>(|state, _| state.logout());
                        });
                        let _ = window_handle.update(cx, |_, window, cx| {
                            router_handle.navigate("/sign-in", window, cx);
                        });
                    }
                    // Network or session startup errors retain the optimistic cached UI.
                    Ok(Err(_)) => {
                        eprintln!("Spotify session restore unavailable; cached UI retained")
                    }
                    Err(_) => {
                        eprintln!("Failed to receive Spotify restore result; cached UI retained")
                    }
                }
            })
            .detach();
        }

        Self {
            router,
            titlebar: cx.new(|cx| Titlebar::new(window, cx)),
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
