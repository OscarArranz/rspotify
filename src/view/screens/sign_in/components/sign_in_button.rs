use crate::lib::auth::{SpotifyAuth, SpotifyAuthConfig, SpotifyToken};
use crate::router::PathRouterHandle;
use crate::state::AppState;

use gpui::{
    AnyElement, App, AsyncApp, InteractiveElement, IntoElement, MouseButton, ParentElement,
    RenderOnce, StatefulInteractiveElement, Styled, Window, div, prelude::*, px, rgb,
};
use std::sync::mpsc;

#[derive(IntoElement)]
pub struct SignInButton {}

impl SignInButton {
    pub fn new() -> Self {
        SignInButton {}
    }

    /// Performs Spotify authentication and returns the token.
    /// This is a blocking call that should be run in a background thread.
    pub fn authenticate() -> Result<SpotifyToken, String> {
        // Load environment variables from .env file
        dotenvy::dotenv().ok();

        let client_id = std::env::var("SPOTIFY_CLIENT_ID")
            .map_err(|_| "SPOTIFY_CLIENT_ID must be set in environment or .env file")?;
        let redirect_uri = std::env::var("SPOTIFY_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:8888/callback".to_string());

        println!("Starting Spotify authentication...");
        println!("A browser window will open for you to authorize the application.");

        let config = SpotifyAuthConfig::new(client_id, redirect_uri);
        let auth = SpotifyAuth::new(config);

        auth.authenticate().map_err(|e| e.to_string())
    }
}

impl RenderOnce for SignInButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Get the path router handle for navigation after auth
        let router_handle = cx.global::<PathRouterHandle>().clone();
        let window_handle = window.window_handle();

        div()
            .id("sign-in-button")
            .bg(rgb(0x1DB954)) // Spotify green
            .hover(|style| style.bg(rgb(0x1ed760)))
            .active(|style| style.bg(rgb(0x169c46)))
            .text_color(rgb(0xFFFFFF))
            .font_weight(gpui::FontWeight::BOLD)
            .px(px(24.0))
            .py(px(12.0))
            .rounded(px(24.0))
            .cursor_pointer()
            .on_mouse_up(MouseButton::Left, move |_, _, cx: &mut App| {
                let router = router_handle.clone();
                let window_handle = window_handle.clone();

                // Use a channel to send the result from the background thread
                let (tx, rx) = mpsc::channel::<Result<SpotifyToken, String>>();

                // Run authentication in a background thread (blocking I/O)
                std::thread::spawn(move || {
                    let result = SignInButton::authenticate();
                    let _ = tx.send(result);
                });

                // Poll for the result using GPUI's async executor
                cx.spawn(async move |cx: &mut AsyncApp| {
                    // Wait for the authentication result
                    let result = rx.recv();

                    match result {
                        Ok(Ok(token)) => {
                            // Store the token in global app state
                            cx.update(|cx| {
                                cx.update_global::<AppState, _>(|state, _cx| {
                                    state.set_auth(&token);
                                    println!("\nAuthentication successful!");
                                    println!("Access Token: {}", token.access_token);
                                    println!("Refresh Token: {}", token.refresh_token);
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
                        Ok(Err(e)) => {
                            eprintln!("Authentication failed: {}", e);
                        }
                        Err(e) => {
                            eprintln!("Failed to receive auth result: {}", e);
                        }
                    }
                })
                .detach();
            })
            .child("Log in with Spotify")
    }
}

impl From<SignInButton> for AnyElement {
    fn from(sign_in_button: SignInButton) -> Self {
        sign_in_button.into_any_element()
    }
}
