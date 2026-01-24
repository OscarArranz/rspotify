//! Resize handle component for dragging between panels.

use gpui::{
    App, CursorStyle, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px, rgb,
};

const HANDLE_WIDTH: f32 = 8.0;
const HANDLE_VISIBLE_WIDTH: f32 = 2.0;
const HANDLE_COLOR: u32 = 0x000000;
const HANDLE_HOVER_COLOR: u32 = 0x7c7c7c;
const HANDLE_ACTIVE_COLOR: u32 = 0xffffff;

/// State for the resize handle.
#[derive(Clone)]
pub struct ResizeHandleState {
    pub is_hovered: bool,
    pub is_dragging: bool,
}

impl Default for ResizeHandleState {
    fn default() -> Self {
        Self {
            is_hovered: false,
            is_dragging: false,
        }
    }
}

/// Creates a resize handle element between two panels.
///
/// # Arguments
/// * `id` - Unique identifier for this handle
/// * `state` - Entity holding the handle's hover/drag state
/// * `on_drag_start` - Callback when drag starts (receives initial mouse x position)
/// * `on_drag_move` - Callback during drag (receives current mouse x position)
/// * `on_drag_end` - Callback when drag ends
pub fn resize_handle<F1, F2>(
    id: impl Into<gpui::ElementId>,
    state: Entity<ResizeHandleState>,
    on_drag_start: F1,
    on_drag_end: F2,
    cx: &mut App,
) -> impl IntoElement
where
    F1: Fn(f32, &mut Window, &mut App) + 'static,
    F2: Fn(&mut Window, &mut App) + 'static,
{
    let is_hovered = state.read(cx).is_hovered;
    let is_dragging = state.read(cx).is_dragging;

    let handle_color = if is_dragging {
        HANDLE_ACTIVE_COLOR
    } else if is_hovered {
        HANDLE_HOVER_COLOR
    } else {
        HANDLE_COLOR
    };

    let state_hover = state.clone();
    let state_down = state.clone();
    let state_up = state.clone();

    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .w(px(HANDLE_WIDTH))
        .h_full()
        .cursor(CursorStyle::OpenHand)
        .on_hover(move |hovered, _window, cx| {
            state_hover.update(cx, |state, cx| {
                state.is_hovered = *hovered;
                cx.notify();
            });
        })
        .when(is_dragging, |this| this.cursor(CursorStyle::ClosedHand))
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let x: f32 = event.position.x.into();
            state_down.update(cx, |state, cx| {
                state.is_dragging = true;
                cx.notify();
            });
            on_drag_start(x, window, cx);
        })
        .on_mouse_up(MouseButton::Left, move |_event, window, cx| {
            let was_dragging = state_up.read(cx).is_dragging;
            if was_dragging {
                state_up.update(cx, |state, cx| {
                    state.is_dragging = false;
                    state.is_hovered = false;
                    cx.notify();
                });
                on_drag_end(window, cx);
            }
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .py(px(4.0))
                .w(px(HANDLE_WIDTH))
                .h_full()
                .child(
                    div()
                        .w(px(HANDLE_VISIBLE_WIDTH))
                        .my(px(8.0))
                        .h_full()
                        .bg(rgb(handle_color))
                        .rounded_full(),
                ),
        )
}
