//! Own scroll input rather than trying to undo GPUI's already-applied wheel movement.
use std::{cell::RefCell, rc::Rc, sync::OnceLock, time::Instant};

const SCROLL_RESPONSE_SECONDS: f32 = 0.018;
const SCROLL_SETTLE_EPSILON_PX: f32 = 0.35;

use gpui::{
    App, DispatchPhase, Div, EntityId, HitboxBehavior, InteractiveElement, IntoElement, ListState,
    Pixels, ScrollHandle, ScrollWheelEvent, Stateful, StatefulInteractiveElement, Styled, Window,
    canvas, point, px,
};

#[derive(Clone, Copy)]
pub(crate) enum ScrollAxis {
    Vertical,
    Horizontal,
}

#[derive(Default)]
struct Motion {
    target: f32,
    last_position: f32,
    running: bool,
    last_frame: Option<Instant>,
    generation: u64,
    frame_intervals_ms: Option<Vec<f32>>,
}

impl Motion {
    fn enqueue(&mut self, current: f32, delta: f32, max: f32) -> bool {
        // A programmatic reset or layout clamp invalidates the old destination.
        if !self.running || (current - self.last_position).abs() > 0.5 {
            self.target = current;
        }
        let pending = self.target - current;
        if pending * delta < 0.0 {
            self.target = current;
        }
        let target = (self.target + delta).clamp(-max.max(0.0), 0.0);
        if (target - current).abs() < 0.01 {
            return false;
        }
        self.target = target;
        self.last_position = current;
        true
    }

    fn advance(&mut self, current: f32, max: f32, seconds: f32) -> Option<f32> {
        if (current - self.last_position).abs() > 0.5 {
            self.running = false;
            return None;
        }
        self.target = self.target.clamp(-max.max(0.0), 0.0);
        let remaining = self.target - current;
        let next = if remaining.abs() < SCROLL_SETTLE_EPSILON_PX {
            self.running = false;
            self.target
        } else {
            current + remaining * (1.0 - (-seconds / SCROLL_RESPONSE_SECONDS).exp())
        };
        self.last_position = next;
        Some(next)
    }
}

#[derive(Clone)]
pub(crate) struct SmoothScrollState(Rc<RefCell<Motion>>, Option<EntityId>);

#[cfg(test)]
impl Default for SmoothScrollState {
    fn default() -> Self {
        Self(Rc::default(), None)
    }
}

impl SmoothScrollState {
    pub(crate) fn new(owner: EntityId) -> Self {
        Self(Rc::default(), Some(owner))
    }

    fn invalidate(&self, window: &mut Window, cx: &mut App) {
        if let Some(owner) = self.1 {
            cx.notify(owner);
        } else {
            // Anonymous states are used by the low-level headless dispatch tests only.
            window.refresh();
        }
    }

    pub(crate) fn cancel(&self) {
        let mut motion = self.0.borrow_mut();
        motion.running = false;
        motion.generation = motion.generation.wrapping_add(1);
    }
}

#[derive(Clone)]
enum Surface {
    Div(ScrollHandle, ScrollAxis),
    List(ListState),
}

impl Surface {
    fn position_and_max(&self) -> (f32, f32) {
        match self {
            Self::Div(handle, ScrollAxis::Vertical) => {
                (handle.offset().y.into(), handle.max_offset().height.into())
            }
            Self::Div(handle, ScrollAxis::Horizontal) => {
                (handle.offset().x.into(), handle.max_offset().width.into())
            }
            Self::List(list) => (
                list.scroll_px_offset_for_scrollbar().y.into(),
                list.max_offset_for_scrollbar().height.into(),
            ),
        }
    }

    fn set_position(&self, position: f32) {
        match self {
            Self::Div(handle, axis) => {
                let mut offset = handle.offset();
                match axis {
                    ScrollAxis::Vertical => offset.y = px(position),
                    ScrollAxis::Horizontal => offset.x = px(position),
                }
                handle.set_offset(offset);
            }
            Self::List(list) => list.set_offset_from_scrollbar(point(px(0.0), px(position))),
        }
    }
}

type OnScroll = Rc<dyn Fn(&mut Window, &mut App)>;

fn handle_wheel(
    event: &ScrollWheelEvent,
    window: &mut Window,
    cx: &mut App,
    surface: &Surface,
    state: &SmoothScrollState,
    on_scroll: &OnScroll,
) {
    if matches!(surface, Surface::Div(_, ScrollAxis::Horizontal)) && !event.modifiers.shift {
        return;
    }
    let delta = event.delta.pixel_delta(px(20.0));
    let delta = match surface {
        Surface::Div(_, ScrollAxis::Horizontal) => {
            if delta.x != Pixels::ZERO {
                delta.x
            } else {
                delta.y
            }
        }
        _ => delta.y,
    };
    if delta == Pixels::ZERO {
        return;
    }
    let (current, max) = surface.position_and_max();
    let mut motion = state.0.borrow_mut();
    if !motion.enqueue(current, delta.into(), max) {
        return;
    }
    cx.stop_propagation();
    if motion.running {
        return;
    }
    motion.running = true;
    motion.generation = motion.generation.wrapping_add(1);
    let generation = motion.generation;
    motion.last_frame = Some(Instant::now());
    static METRICS_ENABLED: OnceLock<bool> = OnceLock::new();
    motion.frame_intervals_ms = (*METRICS_ENABLED
        .get_or_init(|| std::env::var("RSPOTIFY_SCROLL_METRICS").is_ok_and(|value| value == "1")))
    .then(Vec::new);
    drop(motion);
    schedule_frame(
        window,
        surface.clone(),
        state.clone(),
        on_scroll.clone(),
        generation,
    );
    state.invalidate(window, cx);
}

fn schedule_frame(
    window: &mut Window,
    surface: Surface,
    state: SmoothScrollState,
    on_scroll: OnScroll,
    generation: u64,
) {
    window.on_next_frame(move |window, cx| {
        let (current, max) = surface.position_and_max();
        let mut motion = state.0.borrow_mut();
        if !motion.running || motion.generation != generation {
            return;
        }
        let now = Instant::now();
        let elapsed = now
            .duration_since(motion.last_frame.unwrap_or(now))
            .as_secs_f32();
        motion.last_frame = Some(now);
        if let Some(intervals) = &mut motion.frame_intervals_ms {
            intervals.push(elapsed * 1000.0);
        }
        let next = motion.advance(current, max, elapsed);
        let running = motion.running;
        let finished_intervals = if !running {
            motion.frame_intervals_ms.take()
        } else {
            None
        };
        drop(motion);
        if let Some(next) = next {
            surface.set_position(next);
            on_scroll(window, cx);
            state.invalidate(window, cx);
        }
        if running {
            schedule_frame(window, surface, state, on_scroll, generation);
            // Updating the owner above schedules the next frame without discarding all
            // sibling caches. No render-only Window::current_view lookup is needed here.
        }
        if let Some(mut intervals) = finished_intervals.filter(|samples| samples.len() >= 3) {
            // Opt-in aggregate frame-callback spacing, not a CPU/GPU profiler or an FPS claim.
            // Never include URIs, playlist names, search terms, or account information.
            intervals.sort_by(f32::total_cmp);
            let count = intervals.len();
            let mean = intervals.iter().sum::<f32>() / count as f32;
            let p95 = intervals[(count * 95).div_ceil(100).saturating_sub(1)];
            eprintln!(
                "scroll frames: samples={count} mean_ms={mean:.2} p95_ms={p95:.2} max_ms={:.2}",
                intervals[count - 1]
            );
        }
    });
}

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
        // Hidden overflow still supports tracked offsets, but GPUI's default wheel listener
        // no longer moves them. Nested children receive bubble events before this handler.
        let on_scroll: OnScroll = Rc::new(|_, _| {});
        self.overflow_hidden()
            .track_scroll(&handle)
            .on_scroll_wheel(move |event, window, cx| {
                handle_wheel(
                    event,
                    window,
                    cx,
                    &Surface::Div(handle.clone(), axis),
                    &state,
                    &on_scroll,
                );
            })
    }
}

/// Place after the list in a relatively positioned parent. This registers only a scroll
/// listener, leaving track clicks, hover, and input focus to the underlying list.
pub(crate) fn list_scroll_overlay(
    list: ListState,
    state: SmoothScrollState,
    on_scroll: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let on_scroll: OnScroll = Rc::new(on_scroll);
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            let surface = Surface::List(list.clone());
            let state = state.clone();
            let on_scroll = on_scroll.clone();
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.should_handle_scroll(window) {
                    handle_wheel(event, window, cx, &surface, &state, &on_scroll);
                    // At the list boundary suppress its native handler as well: otherwise
                    // it could jump independently of our animation.
                    if event.delta.pixel_delta(px(20.0)).y != Pixels::ZERO {
                        cx.stop_propagation();
                    }
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, Context, ParentElement, Render, TestAppContext, div, list, size};

    struct TestSurface {
        handle: ScrollHandle,
        state: SmoothScrollState,
        axis: ScrollAxis,
    }

    impl Render for TestSurface {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let (width, height) = match self.axis {
                ScrollAxis::Vertical => (100.0, 1000.0),
                ScrollAxis::Horizontal => (1000.0, 100.0),
            };
            div()
                .id("scroll-test")
                .size_full()
                .smooth_scroll(self.handle.clone(), self.axis, self.state.clone())
                .child(div().w(px(width)).h(px(height)).flex_shrink_0())
        }
    }

    #[gpui::test]
    fn pixel_and_line_events_are_intercepted_without_a_native_jump(cx: &mut TestAppContext) {
        for axis in [ScrollAxis::Vertical, ScrollAxis::Horizontal] {
            let cx = cx.add_empty_window();
            let handle = ScrollHandle::new();
            let state = SmoothScrollState::default();
            cx.draw(
                point(px(0.0), px(0.0)),
                size(px(100.0), px(100.0)),
                |_, cx| {
                    cx.new(|_| TestSurface {
                        handle: handle.clone(),
                        state: state.clone(),
                        axis,
                    })
                },
            );
            cx.simulate_event(ScrollWheelEvent {
                position: point(px(10.0), px(10.0)),
                delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-60.0))),
                modifiers: gpui::Modifiers {
                    shift: matches!(axis, ScrollAxis::Horizontal),
                    ..Default::default()
                },
                ..Default::default()
            });
            assert_eq!(handle.offset(), point(px(0.0), px(0.0)));
            assert_eq!(state.0.borrow().target, -60.0);
            assert!(state.0.borrow().running);
            // draw() installs a temporary test element, not the empty window's root.
            // Reinstall it after simulate_event's refresh before dispatching again.
            cx.draw(
                point(px(0.0), px(0.0)),
                size(px(100.0), px(100.0)),
                |_, cx| {
                    cx.new(|_| TestSurface {
                        handle: handle.clone(),
                        state: state.clone(),
                        axis,
                    })
                },
            );
            cx.simulate_event(ScrollWheelEvent {
                position: point(px(10.0), px(10.0)),
                delta: gpui::ScrollDelta::Lines(point(0.0, -3.0)),
                modifiers: gpui::Modifiers {
                    shift: matches!(axis, ScrollAxis::Horizontal),
                    ..Default::default()
                },
                ..Default::default()
            });
            assert_eq!(state.0.borrow().target, -120.0);
            state.cancel();
        }
    }

    #[gpui::test]
    fn playlist_overlay_prevents_native_jump(cx: &mut TestAppContext) {
        struct TestList(ListState, SmoothScrollState, Rc<std::cell::Cell<usize>>);
        impl Render for TestList {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let clicks = self.2.clone();
                div()
                    .relative()
                    .size(px(100.0))
                    .child(
                        list(self.0.clone(), move |index, _, _| {
                            let clicks = clicks.clone();
                            div()
                                .id(("track", index))
                                .h(px(56.0))
                                .w_full()
                                .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
                                .into_any_element()
                        })
                        .size_full(),
                    )
                    .child(list_scroll_overlay(
                        self.0.clone(),
                        self.1.clone(),
                        |_, _| {},
                    ))
            }
        }
        let list = ListState::new(100, gpui::ListAlignment::Top, px(200.0));
        let state = SmoothScrollState::default();
        let clicks = Rc::new(std::cell::Cell::new(0));
        let (view, cx) =
            cx.add_window_view(|_, _| TestList(list.clone(), state.clone(), clicks.clone()));
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(100.0), px(100.0)),
            |_, _| view.clone(),
        );
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(10.0), px(10.0)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-60.0))),
            ..Default::default()
        });
        assert_eq!(list.scroll_px_offset_for_scrollbar().y, px(0.0));
        assert_eq!(state.0.borrow().target, -60.0);
        assert!(state.0.borrow().running);
        state.cancel();
        // Exercise the same incremental offsets against a real virtualized ListState.
        let surface = Surface::List(list.clone());
        let mut motion = Motion::default();
        motion.enqueue(0.0, -60.0, surface.position_and_max().1);
        motion.running = true;
        let mut positions = Vec::new();
        for _ in 0..40 {
            let (current, max) = surface.position_and_max();
            let next = motion.advance(current, max, 0.016).unwrap();
            surface.set_position(next);
            positions.push(list.scroll_px_offset_for_scrollbar().y);
        }
        assert!(positions[0] < px(0.0) && positions[0] > px(-60.0));
        assert_eq!(positions.last(), Some(&px(-60.0)));
        assert_eq!(list.logical_scroll_top().item_ix, 1);
        assert!(
            list.bounds_for_item(99).is_none(),
            "distant rows remain unrendered"
        );
        list.scroll_to(gpui::ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        });
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(100.0), px(100.0)),
            |_, _| view.clone(),
        );
        cx.simulate_click(point(px(10.0), px(10.0)), Default::default());
        assert_eq!(
            clicks.get(),
            1,
            "scroll overlay must not intercept track clicks"
        );
    }

    #[gpui::test]
    fn nested_carousel_consumes_input_then_chains_at_boundary(cx: &mut TestAppContext) {
        struct Nested {
            outer: ScrollHandle,
            inner: ScrollHandle,
            outer_motion: SmoothScrollState,
            inner_motion: SmoothScrollState,
        }
        impl Render for Nested {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .id("outer")
                    .size_full()
                    .flex()
                    .flex_col()
                    .smooth_scroll(
                        self.outer.clone(),
                        ScrollAxis::Vertical,
                        self.outer_motion.clone(),
                    )
                    .child(
                        div()
                            .id("inner")
                            .w_full()
                            .h(px(50.0))
                            .flex_shrink_0()
                            .smooth_scroll(
                                self.inner.clone(),
                                ScrollAxis::Horizontal,
                                self.inner_motion.clone(),
                            )
                            .child(div().w(px(1000.0)).h(px(50.0)).flex_shrink_0()),
                    )
                    .child(div().w_full().h(px(1000.0)).flex_shrink_0())
            }
        }
        let cx = cx.add_empty_window();
        let outer = ScrollHandle::new();
        let inner = ScrollHandle::new();
        let outer_motion = SmoothScrollState::default();
        let inner_motion = SmoothScrollState::default();
        let view = cx.new(|_| Nested {
            outer: outer.clone(),
            inner: inner.clone(),
            outer_motion: outer_motion.clone(),
            inner_motion: inner_motion.clone(),
        });
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(100.0), px(100.0)),
            |_, _| view.clone(),
        );
        let wheel = ScrollWheelEvent {
            position: point(px(10.0), px(10.0)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-60.0))),
            modifiers: gpui::Modifiers {
                shift: true,
                ..Default::default()
            },
            ..Default::default()
        };
        cx.simulate_event(wheel.clone());
        assert_eq!(inner_motion.0.borrow().target, -60.0);
        assert!(!outer_motion.0.borrow().running);
        inner_motion.cancel();
        inner.set_offset(point(-inner.max_offset().width, px(0.0)));
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(100.0), px(100.0)),
            |_, _| view.clone(),
        );
        cx.simulate_event(wheel);
        assert_eq!(outer_motion.0.borrow().target, -60.0);
        assert!(!inner_motion.0.borrow().running);
        outer_motion.cancel();
    }

    #[test]
    fn events_accumulate_and_reverse_without_stale_momentum() {
        let mut m = Motion::default();
        assert!(m.enqueue(0.0, -60.0, 500.0));
        m.running = true;
        assert!(m.enqueue(0.0, -60.0, 500.0));
        assert_eq!(m.target, -120.0);
        let position = m.advance(0.0, 500.0, 0.016).unwrap();
        assert!(position < 0.0 && position > -120.0);
        assert!(m.enqueue(position, 20.0, 500.0));
        assert_eq!(m.target, (position + 20.0).min(0.0));
    }

    #[test]
    fn boundaries_and_programmatic_resets() {
        let mut m = Motion::default();
        assert!(!m.enqueue(0.0, 60.0, 500.0));
        assert!(!m.enqueue(-500.0, -60.0, 500.0));
        assert!(m.enqueue(-480.0, -60.0, 500.0));
        assert_eq!(m.target, -500.0);
        m.running = true;
        assert!(m.advance(0.0, 500.0, 0.016).is_none());
        assert!(!m.running);
    }

    #[test]
    fn easing_is_frame_rate_independent_and_settles() {
        fn run(dt: f32, frames: usize) -> f32 {
            let mut m = Motion::default();
            m.enqueue(0.0, -60.0, 500.0);
            m.running = true;
            let mut position = 0.0;
            for _ in 0..frames {
                position = m.advance(position, 500.0, dt).unwrap();
            }
            position
        }
        assert!((run(0.016, 6) - run(0.008, 12)).abs() < 0.001);
        assert_eq!(run(0.016, 40), -60.0);

        let mut m = Motion::default();
        m.enqueue(0.0, -60.0, 500.0);
        m.running = true;
        let mut position = 0.0;
        for _ in 0..6 {
            position = m.advance(position, 500.0, 0.016).unwrap();
        }
        assert!(
            (position + 60.0).abs() < 1.0,
            "should settle near the target in ~100ms"
        );
    }
}
