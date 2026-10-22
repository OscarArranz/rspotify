//! Explicitly sized GPUI cache boundaries for independently updating UI regions.
use gpui::{AnyView, StyleRefinement, Styled, px};

pub(crate) fn cached_fill(view: impl Into<AnyView>) -> AnyView {
    view.into().cached(StyleRefinement::default().size_full())
}

pub(crate) fn cached_bar(view: impl Into<AnyView>, height: f32) -> AnyView {
    view.into().cached(
        StyleRefinement::default()
            .w_full()
            .h(px(height))
            .flex_shrink_0(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::components::smooth_scroll::{
        ScrollAxis, SmoothScrollElement, SmoothScrollState,
    };
    use gpui::InteractiveElement;
    use gpui::{
        AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext, Window,
        div,
    };
    use std::{cell::Cell, rc::Rc};

    struct Region(Rc<Cell<usize>>, gpui::ScrollHandle, SmoothScrollState);
    impl Render for Region {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0.set(self.0.get() + 1);
            div()
                .id("region-scroll")
                .size_full()
                .smooth_scroll(self.1.clone(), ScrollAxis::Vertical, self.2.clone())
                .child(div().w_full().h(px(2000.0)).flex_shrink_0())
        }
    }

    struct Root {
        first: Entity<Region>,
        second: Entity<Region>,
    }
    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(cached_bar(self.first.clone(), 76.0))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .child(cached_fill(self.second.clone())),
                )
        }
    }

    #[gpui::test]
    fn cached_regions_reuse_clean_siblings_and_invalidate_on_notify_and_resize(
        cx: &mut TestAppContext,
    ) {
        let first_count = Rc::new(Cell::new(0));
        let second_count = Rc::new(Cell::new(0));
        let (root, cx) = cx.add_window_view(|_, cx| Root {
            first: cx.new(|cx| {
                Region(
                    first_count.clone(),
                    gpui::ScrollHandle::new(),
                    SmoothScrollState::new(cx.entity_id()),
                )
            }),
            second: cx.new(|cx| {
                Region(
                    second_count.clone(),
                    gpui::ScrollHandle::new(),
                    SmoothScrollState::new(cx.entity_id()),
                )
            }),
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let initial = (first_count.get(), second_count.get());
        assert!(initial.0 > 0 && initial.1 > 0);

        // Notify the root to force a frame without marking either cached child dirty.
        root.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert_eq!((first_count.get(), second_count.get()), initial);

        root.update(cx, |root, cx| root.first.update(cx, |_, cx| cx.notify()));
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(first_count.get() > initial.0);
        assert_eq!(second_count.get(), initial.1);

        let before_scroll = (first_count.get(), second_count.get());
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(10.0), px(10.0)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(-60.0))),
            ..Default::default()
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(first_count.get() > before_scroll.0);
        assert_eq!(
            second_count.get(),
            before_scroll.1,
            "scroll owner notification must preserve its sibling's cached rendering"
        );
        root.update(cx, |root, cx| {
            root.first.update(cx, |region, _| region.2.cancel())
        });

        let before_resize = (first_count.get(), second_count.get());
        cx.simulate_resize(gpui::size(px(700.0), px(500.0)));
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(first_count.get() > before_resize.0);
        assert!(second_count.get() > before_resize.1);
    }
}
