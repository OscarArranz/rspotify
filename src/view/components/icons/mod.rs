//! Placeholder icon components.
//!
//! Simple placeholder icons that can be replaced with actual SVG icons later.

use gpui::{
    App, Hsla, IntoElement, Length, ParentElement, RenderOnce, Styled, Window, div, px, rgb, svg,
};

const FRIENDS_OUTLINED_SVG: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/assets/friends-outlined.svg");
const NOTIFICATION_BELL_SVG: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/notification-bell-outlined.svg"
);
const BACK_SVG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/back.svg");
const FORWARD_SVG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/forward.svg");
const HOME_SVG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/home-outlined.svg");
const HOME_ACTIVE_SVG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/home-full.svg");

/// A placeholder icon (+ shape) used as a stand-in for all icons.
/// Replace with actual SVG icons as needed.
#[derive(IntoElement, Clone)]
pub struct PlaceholderIcon {
    size: f32,
    color: gpui::Hsla,
}

impl PlaceholderIcon {
    pub fn new() -> Self {
        Self {
            size: 16.0,
            color: gpui::white(),
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: gpui::Hsla) -> Self {
        self.color = color;
        self
    }
}

impl RenderOnce for PlaceholderIcon {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // Simple + shape using divs
        let size = px(self.size);
        let stroke_width = px(2.0);

        div()
            .size(size)
            .relative()
            // Horizontal line
            .child(
                div()
                    .absolute()
                    .top(px(self.size / 2.0 - 1.0))
                    .left_0()
                    .w_full()
                    .h(stroke_width)
                    .bg(self.color)
                    .rounded(px(1.0)),
            )
            // Vertical line
            .child(
                div()
                    .absolute()
                    .left(px(self.size / 2.0 - 1.0))
                    .top_0()
                    .h_full()
                    .w(stroke_width)
                    .bg(self.color)
                    .rounded(px(1.0)),
            )
    }
}

// Convenience type aliases for semantic icon usage
// All currently use PlaceholderIcon but can be replaced with actual icons

pub type MenuIcon = PlaceholderIcon;
pub type BackIcon = PlaceholderIcon;
pub type ForwardIcon = PlaceholderIcon;
pub type HomeIcon = PlaceholderIcon;
pub type SearchIcon = PlaceholderIcon;
pub type BrowseIcon = PlaceholderIcon;
pub type MinimizeIcon = PlaceholderIcon;
pub type MaximizeIcon = PlaceholderIcon;
pub type CloseIcon = PlaceholderIcon;
pub type UserIcon = PlaceholderIcon;

pub fn icon(
    icon_path: &str,
    is_hovered: bool,
    base_color: impl Into<Hsla>,
    hover_color: impl Into<Hsla>,
    size: impl Clone + Into<Length>,
) -> impl IntoElement {
    let base_icon = svg().path(icon_path.to_string()).size(size.clone());

    if is_hovered {
        base_icon.text_color(hover_color)
    } else {
        base_icon.text_color(base_color)
    }
}

pub fn icon_with_hover_and_active_state(
    icon_path: &str,
    active_icon_path: &str,
    is_hovered: bool,
    is_active: bool,
    base_color: impl Into<Hsla>,
    hover_color: impl Into<Hsla>,
    size: impl Clone + Into<Length>,
) -> impl IntoElement {
    let base_icon = svg().path(icon_path.to_string()).size(size.clone());
    let active_icon = svg().path(active_icon_path.to_string()).size(size.clone());

    if is_hovered {
        base_icon.text_color(hover_color)
    } else if is_active {
        active_icon.text_color(hover_color)
    } else {
        base_icon.text_color(base_color)
    }
}

pub fn home_icon(is_hovered: bool, is_active: bool) -> impl IntoElement {
    icon_with_hover_and_active_state(
        HOME_SVG,
        HOME_ACTIVE_SVG,
        is_hovered,
        is_active,
        rgb(0xb3b3b3),
        gpui::white(),
        px(24.0),
    )
}

fn titlebar_right_icon(icon_path: &str, is_hovered: bool) -> impl IntoElement {
    icon(
        icon_path,
        is_hovered,
        rgb(0x969696),
        gpui::white(),
        px(16.0),
    )
}

fn titlebar_nav_button(icon_path: &str, is_hovered: bool) -> impl IntoElement {
    icon(
        icon_path,
        is_hovered,
        rgb(0x363636),
        gpui::white(),
        px(11.0),
    )
}

pub fn back_icon(is_hovered: bool) -> impl IntoElement {
    titlebar_nav_button(BACK_SVG, is_hovered)
}

pub fn forward_icon(is_hovered: bool) -> impl IntoElement {
    titlebar_nav_button(FORWARD_SVG, is_hovered)
}

pub fn notification_bell_icon(is_hovered: bool) -> impl IntoElement {
    titlebar_right_icon(NOTIFICATION_BELL_SVG, is_hovered)
}

pub fn friends_outlined_icon(is_hovered: bool) -> impl IntoElement {
    titlebar_right_icon(FRIENDS_OUTLINED_SVG, is_hovered)
}
