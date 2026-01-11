mod components;

use components::sign_in_button::SignInButton;
use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, rgb};

/// Sign-in screen shown when user is not authenticated.
pub struct SignInScreen;

impl SignInScreen {
    pub fn new() -> Self {
        Self
    }
}

impl Render for SignInScreen {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .size_full()
            .bg(rgb(0x121212)) // Spotify dark background
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .items_center()
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0xFFFFFF))
                            .child("Welcome to Spotify"),
                    )
                    .child(
                        div()
                            .text_color(rgb(0xb3b3b3))
                            .child("Sign in to access your music"),
                    )
                    .child(SignInButton::new()),
            )
    }
}
