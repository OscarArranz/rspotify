use std::{cell::Cell, rc::Rc, time::Instant};

use gpui::{
    Div, EntityId, InteractiveElement, Pixels, Point, ScrollHandle, ScrollWheelEvent, Stateful,
    StatefulInteractiveElement, Window, point,
};

const WHEEL_ANIMATION: std::time::Duration = std::time::Duration::from_millis(150);

#[derive(Clone, Copy)]
pub(crate) enum ScrollAxis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Default)]
pub(crate) struct SmoothScrollState(Rc<Cell<u64>>);

pub(crate) trait SmoothScrollElement {
    fn smooth_scroll(
        self,
        handle: ScrollHandle,
        axis: ScrollAxis,
        state: SmoothScrollState,
    ) -> Self;
}

impl SmoothScrollElement for Stateful<Div> {
    fn smooth_scroll(
        self,
        handle: ScrollHandle,
        axis: ScrollAxis,
        state: SmoothScrollState,
    ) -> Self {
        self.track_scroll(&handle)
            .on_scroll_wheel(move |event, window, _cx| {
                smooth_wheel_delta(event, window, &handle, axis, &state.0);
            })
    }
}

fn smooth_wheel_delta(
    event: &ScrollWheelEvent,
    window: &mut Window,
    handle: &ScrollHandle,
    axis: ScrollAxis,
    generation: &Rc<Cell<u64>>,
) {
    // Precise touchpad input is already delivered as small, continuous pixel deltas.
    if event.delta.precise() {
        return;
    }

    let delta = event.delta.pixel_delta(window.line_height());
    let (dx, dy) = match axis {
        ScrollAxis::Vertical => (Pixels::ZERO, delta.y),
        ScrollAxis::Horizontal => (
            if delta.x == Pixels::ZERO {
                delta.y
            } else {
                delta.x
            },
            Pixels::ZERO,
        ),
    };
    if dx == Pixels::ZERO && dy == Pixels::ZERO {
        return;
    }

    // GPUI applies the native wheel delta before bubbling to this handler. Rewind that
    // single step, then animate to its destination over successive rendered frames.
    let target = handle.offset();
    let start = point(target.x - dx, target.y - dy);
    if start == target {
        return;
    }

    let next_generation = generation.get().wrapping_add(1);
    generation.set(next_generation);
    handle.set_offset(start);

    let animation = WheelAnimation {
        handle: handle.clone(),
        start,
        target,
        started: Instant::now(),
        generation: generation.clone(),
        expected_generation: next_generation,
        view: window.current_view(),
    };
    schedule_animation_frame(window, animation);
}

struct WheelAnimation {
    handle: ScrollHandle,
    start: Point<Pixels>,
    target: Point<Pixels>,
    started: Instant,
    generation: Rc<Cell<u64>>,
    expected_generation: u64,
    view: EntityId,
}

fn schedule_animation_frame(window: &mut Window, animation: WheelAnimation) {
    window.on_next_frame(move |window, cx| {
        if animation.generation.get() != animation.expected_generation {
            return;
        }

        let progress = (animation.started.elapsed().as_secs_f32() / WHEEL_ANIMATION.as_secs_f32())
            .clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        animation.handle.set_offset(point(
            interpolate(animation.start.x, animation.target.x, eased),
            interpolate(animation.start.y, animation.target.y, eased),
        ));
        cx.notify(animation.view);

        if progress < 1.0 {
            let next = WheelAnimation {
                handle: animation.handle,
                start: animation.start,
                target: animation.target,
                started: animation.started,
                generation: animation.generation,
                expected_generation: animation.expected_generation,
                view: animation.view,
            };
            window.request_animation_frame();
            schedule_animation_frame(window, next);
        }
    });
}

fn interpolate(start: Pixels, target: Pixels, progress: f32) -> Pixels {
    start + (target - start) * progress
}
