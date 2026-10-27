//! SVG icon helpers used by the titlebar.

use gpui::{Hsla, IntoElement, Length, Styled, px, rgb, svg};

const FRIENDS_OUTLINED_SVG: &str = "icons/friends-outlined.svg";
const NOTIFICATION_BELL_SVG: &str = "icons/notification-bell-outlined.svg";
const BACK_SVG: &str = "icons/back.svg";
const FORWARD_SVG: &str = "icons/forward.svg";
const HOME_SVG: &str = "icons/home-outlined.svg";
const HOME_ACTIVE_SVG: &str = "icons/home-full.svg";

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
