//! Window control buttons (minimize, maximize, close) component.

use gpui::{
    App, InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, Styled, Window,
    WindowControlArea, div, prelude::FluentBuilder, px, rgb,
};

use super::{BUTTON_HOVER_BG, CLOSE_BUTTON_HOVER_BG, TITLEBAR_HEIGHT};

/// Window control buttons (minimize, maximize, close).
#[derive(IntoElement)]
pub(super) struct WindowControls;

impl WindowControls {
    pub(super) fn new() -> Self {
        Self
    }

    fn get_font() -> &'static str {
        use windows::Wdk::System::SystemServices::RtlGetVersion;

        let mut version = unsafe { std::mem::zeroed() };
        let status = unsafe { RtlGetVersion(&mut version) };

        if status.is_ok() && version.dwBuildNumber >= 22000 {
            "Segoe Fluent Icons"
        } else {
            "Segoe MDL2 Assets"
        }
    }
}

impl RenderOnce for WindowControls {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let is_windows = cfg!(target_os = "windows");

        div()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .flex()
            .items_center()
            .font_family(Self::get_font())
            .text_size(px(10.0))
            .text_color(gpui::white())
            // Minimize button
            .child(
                div()
                    .id("minimize-btn")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(45.0))
                    .h(px(TITLEBAR_HEIGHT))
                    .occlude()
                    .hover(|style| style.bg(rgb(BUTTON_HOVER_BG)))
                    .when(is_windows, |this| {
                        this.window_control_area(WindowControlArea::Min)
                    })
                    .child("\u{e921}"),
            )
            // Maximize button
            .child(
                div()
                    .id("maximize-btn")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(46.0))
                    .h(px(TITLEBAR_HEIGHT))
                    .hover(|style| style.bg(rgb(BUTTON_HOVER_BG)))
                    .occlude()
                    .when(is_windows, |this| {
                        this.window_control_area(WindowControlArea::Max)
                    })
                    // .on_mouse_up(MouseButton::Left, move |_, window, _cx| {
                    //     window.zoom_window();
                    // })
                    .map(|this| {
                        this.child(if window.is_maximized() {
                            "\u{e923}"
                        } else {
                            "\u{e922}"
                        })
                    }),
            )
            // Close button
            .child(
                div()
                    .id("close-btn")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(44.0))
                    .h(px(TITLEBAR_HEIGHT))
                    .occlude()
                    .hover(|style| style.bg(rgb(CLOSE_BUTTON_HOVER_BG)))
                    .when(is_windows, |this| {
                        this.window_control_area(WindowControlArea::Close)
                    })
                    .child("\u{e8bb}"),
            )
    }
}
