use crate::lib::spotify::NativeSpotify;
use crate::router::PathRouterHandle;
use crate::state::AppState;

use futures::channel::oneshot;
use gpui::{
    AnyElement, App, AsyncApp, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement,
    RenderOnce, StatefulInteractiveElement, Styled, Window, div, prelude::*, px, rgb,
};

#[derive(Clone, Default)]
enum SignInStatus {
    #[default]
    Idle,
    Loading,
    Error(String),
}

#[derive(Default)]
struct SignInState {
    status: SignInStatus,
}

#[derive(IntoElement)]
pub struct SignInButton {}

impl SignInButton {
    pub fn new() -> Self {
        SignInButton {}
    }

    /// Performs Spotify authentication and returns the authenticated native client.
    /// This is a blocking call that should be run in a background thread.
    pub fn do_authenticate() -> Result<NativeSpotify, String> {
        NativeSpotify::authenticate().map_err(|e| e.to_string())
    }
}

impl RenderOnce for SignInButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Get the path router handle for navigation after auth
        let router_handle = cx.global::<PathRouterHandle>().clone();
        let window_handle = window.window_handle();
        let state: Entity<SignInState> = window.use_state(cx, |_, _| SignInState::default());
        let status = state.read(cx).status.clone();

        let (label, disabled) = match &status {
            SignInStatus::Idle => ("Log in with Spotify", false),
            SignInStatus::Loading => ("Waiting for Spotify…", true),
            SignInStatus::Error(_) => ("Try again", false),
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .id("sign-in-button")
                    .bg(if disabled {
                        rgb(0x169c46)
                    } else {
                        rgb(0x1DB954)
                    })
                    .when(!disabled, |this| {
                        this.hover(|style| style.bg(rgb(0x1ed760)))
                            .active(|style| style.bg(rgb(0x169c46)))
                    })
                    .text_color(rgb(0xFFFFFF))
                    .font_weight(gpui::FontWeight::BOLD)
                    .px(px(24.0))
                    .py(px(12.0))
                    .rounded(px(24.0))
                    .when(!disabled, |this| this.cursor_pointer())
                    .on_mouse_up(MouseButton::Left, {
                        let state = state.clone();
                        let router_handle = router_handle.clone();
                        let window_handle = window_handle.clone();
                        move |_, _, cx: &mut App| {
                            if matches!(state.read(cx).status, SignInStatus::Loading) {
                                return;
                            }
                            state.update(cx, |state, cx| {
                                state.status = SignInStatus::Loading;
                                cx.notify();
                            });

                            let router = router_handle.clone();
                            let window_handle = window_handle.clone();
                            let async_state = state.clone();
                            let (tx, rx) = oneshot::channel::<
                                Result<
                                    (NativeSpotify, Option<crate::lib::spotify::UserProfile>),
                                    String,
                                >,
                            >();

                            // Run authentication in a background thread (blocking I/O)
                            std::thread::spawn(move || {
                                let result = SignInButton::do_authenticate().map(|client| {
                                    let profile = client.persist_profile();
                                    (client, profile)
                                });
                                let _ = tx.send(result);
                            });

                            // Wait for the result without blocking the GPUI foreground thread.
                            cx.spawn(async move |cx: &mut AsyncApp| {
                                let result = rx.await;

                                match result {
                                    Ok(Ok((spotify_client, profile))) => {
                                        // Store the authenticated client in global app state
                                        cx.update(|cx| {
                                            cx.update_global::<AppState, _>(|state, _cx| {
                                                state.set_spotify_client(spotify_client);
                                                if let Some(profile) = profile {
                                                    state.set_profile(profile);
                                                }
                                            });
                                        })
                                        .ok();

                                        // Navigate to main layout
                                        window_handle
                                            .update(cx, |_, window, cx| {
                                                router.navigate("/main", window, cx);
                                            })
                                            .ok();
                                    }
                                    Ok(Err(error)) => {
                                        eprintln!("Authentication failed: {error}");
                                        let _ = async_state.update(cx, |state, cx| {
                                            state.status = SignInStatus::Error(error);
                                            cx.notify();
                                        });
                                    }
                                    Err(error) => {
                                        eprintln!("Failed to receive auth result: {error}");
                                        let _ = async_state.update(cx, |state, cx| {
                                            state.status = SignInStatus::Error(error.to_string());
                                            cx.notify();
                                        });
                                    }
                                }
                            })
                            .detach();
                        }
                    })
                    .child(label),
            )
            .children(match status {
                SignInStatus::Error(error) => Some(
                    div()
                        .max_w(px(520.0))
                        .text_sm()
                        .text_color(rgb(0xf15e6d))
                        .child(error),
                ),
                _ => None,
            })
    }
}

impl From<SignInButton> for AnyElement {
    fn from(sign_in_button: SignInButton) -> Self {
        sign_in_button.into_any_element()
    }
}
