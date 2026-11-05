//! Fixed-width, lazily rendered carousels inside an ordinary scrolling `Div`.
//!
//! The parent retains scrolling, clipping, input handling and its `ScrollHandle`.
//! Only the content extent lives here; visible card positions use the parent's
//! actual content mask, including clipping by ancestors further up the tree.
use std::ops::Range;

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Context, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window, point, px, size,
};

const OVERSCAN: usize = 2;

type RenderItem = dyn Fn(usize, &mut Window, &mut App) -> AnyElement;

/// Make this the sole child of the existing horizontal scrolling container.
/// `item_width` includes the card's padding; `gap` is only between cards.
/// Cards keep their own styling and event handlers. The representative card is
/// measured at the fixed width on each layout, so font/style/width changes do
/// not leave a cached height behind. Card heights should share a row geometry;
/// use `with_measurement_index` when an optional text row needs a representative.
pub(crate) fn virtual_horizontal_list<V: 'static>(
    view: Entity<V>,
    id: impl Into<ElementId>,
    count: usize,
    item_width: f32,
    gap: f32,
    render: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) -> AnyElement + 'static,
) -> VirtualHorizontalList {
    assert!(item_width.is_finite() && item_width > 0.0);
    assert!(gap.is_finite() && gap >= 0.0);
    VirtualHorizontalList {
        id: id.into(),
        count,
        item_width,
        gap,
        measurement_index: 0,
        render: Box::new(move |index, window, cx| {
            view.update(cx, |view, cx| render(view, index, window, cx))
        }),
    }
}

pub(crate) struct VirtualHorizontalList {
    id: ElementId,
    count: usize,
    item_width: f32,
    gap: f32,
    measurement_index: usize,
    render: Box<RenderItem>,
}

impl VirtualHorizontalList {
    /// Select a representative with the shelf's full text-row structure.
    /// Choose this from model metadata when it changes, not by constructing all
    /// cards during rendering. An out-of-range index is clamped after a shrink.
    pub(crate) fn with_measurement_index(mut self, index: usize) -> Self {
        self.measurement_index = index;
        self
    }

    fn content_width(&self) -> f32 {
        if self.count == 0 {
            0.0
        } else {
            self.item_width * self.count as f32 + self.gap * (self.count - 1) as f32
        }
    }

    fn visible_range(&self, bounds: Bounds<Pixels>, clip: Bounds<Pixels>) -> Range<usize> {
        if self.count == 0 || !bounds.intersects(&clip) {
            return 0..0;
        }
        let left = f32::from(clip.left().max(bounds.left()) - bounds.left());
        let right = f32::from(clip.right().min(bounds.right()) - bounds.left());
        if right <= left {
            return 0..0;
        }
        let stride = self.item_width + self.gap;
        let start = (left / stride).floor() as usize;
        let end = (right / stride).ceil() as usize;
        start.saturating_sub(OVERSCAN)..end.saturating_add(OVERSCAN).min(self.count)
    }
}

pub(crate) struct HorizontalLayout {
    // Reuse the measured card if it is visible instead of invoking its callback
    // a second time. A distant scroll still only measures this one extra card.
    representative: Option<(usize, AnyElement)>,
    visible: Vec<AnyElement>,
}

impl IntoElement for VirtualHorizontalList {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for VirtualHorizontalList {
    type RequestLayoutState = HorizontalLayout;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, HorizontalLayout) {
        let mut representative = None;
        let height = if self.count == 0 {
            px(0.0)
        } else {
            let index = self.measurement_index.min(self.count - 1);
            let mut item = (self.render)(index, window, cx);
            let measured = item.layout_as_root(
                size(
                    AvailableSpace::Definite(px(self.item_width)),
                    AvailableSpace::MaxContent,
                ),
                window,
                cx,
            );
            representative = Some((index, item));
            measured.height
        };
        let layout = window.request_layout(
            Style {
                size: size(px(self.content_width()).into(), height.into()),
                flex_shrink: 0.0,
                ..Default::default()
            },
            [],
            cx,
        );
        (
            layout,
            HorizontalLayout {
                representative,
                visible: Vec::new(),
            },
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut HorizontalLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        let range = self.visible_range(bounds, window.content_mask().bounds);
        state.visible.reserve(range.len());
        for index in range {
            let mut item = if state
                .representative
                .as_ref()
                .is_some_and(|(measured, _)| *measured == index)
            {
                state.representative.take().unwrap().1
            } else {
                let mut item = (self.render)(index, window, cx);
                item.layout_as_root(
                    size(
                        AvailableSpace::Definite(px(self.item_width)),
                        AvailableSpace::Definite(bounds.size.height),
                    ),
                    window,
                    cx,
                );
                item
            };
            item.prepaint_at(
                bounds.origin + point(px(index as f32 * (self.item_width + self.gap)), px(0.0)),
                window,
                cx,
            );
            state.visible.push(item);
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        state: &mut HorizontalLayout,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        for item in &mut state.visible {
            item.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    use gpui::{
        InteractiveElement, ParentElement, Render, ScrollHandle, StatefulInteractiveElement,
        Styled, TestAppContext, canvas, div,
    };

    #[derive(Default)]
    struct Observations {
        created: Vec<usize>,
        laid_out: Vec<(usize, Bounds<Pixels>)>,
        clicked: Vec<usize>,
    }

    struct Carousel {
        count: usize,
        width: f32,
        card_width: f32,
        card_height: f32,
        gap: f32,
        clip_width: Option<f32>,
        measurement_index: usize,
        handle: ScrollHandle,
        observed: Rc<RefCell<Observations>>,
    }

    impl Carousel {
        fn large(handle: ScrollHandle, observed: Rc<RefCell<Observations>>) -> Self {
            Self {
                count: 1000,
                width: 250.0,
                card_width: 100.0,
                card_height: 40.0,
                gap: 10.0,
                clip_width: None,
                measurement_index: 0,
                handle,
                observed,
            }
        }
    }

    impl Render for Carousel {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            // A headless draw may run several frames; observe the latest frame.
            {
                let mut observed = self.observed.borrow_mut();
                observed.created.clear();
                observed.laid_out.clear();
            }
            div()
                .w(px(self.clip_width.unwrap_or(self.width)))
                .overflow_hidden()
                .child(
                    div()
                        .id("carousel-scroll")
                        .flex()
                        .flex_shrink_0()
                        .w(px(self.width))
                        .overflow_x_scroll()
                        .track_scroll(&self.handle)
                        .child(
                            virtual_horizontal_list(
                                cx.entity(),
                                "virtual-cards",
                                self.count,
                                self.card_width,
                                self.gap,
                                |this, index, _, _| {
                                    this.observed.borrow_mut().created.push(index);
                                    let layout = this.observed.clone();
                                    let clicks = this.observed.clone();
                                    div()
                                        .id(("card", index))
                                        .w(px(this.card_width))
                                        .h(px(this.card_height))
                                        .on_click(move |_, _, _| {
                                            clicks.borrow_mut().clicked.push(index);
                                        })
                                        .child(
                                            canvas(
                                                move |bounds, _, _| {
                                                    layout
                                                        .borrow_mut()
                                                        .laid_out
                                                        .push((index, bounds));
                                                },
                                                |_, _, _, _| {},
                                            )
                                            .size_full(),
                                        )
                                        .into_any_element()
                                },
                            )
                            .with_measurement_index(self.measurement_index),
                        ),
                )
        }
    }

    #[gpui::test]
    fn bounds_creation_and_scrolls_directly_to_distant_cards(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let observed = Rc::new(RefCell::new(Observations::default()));
        let (view, cx) =
            cx.add_window_view(|_, _| Carousel::large(handle.clone(), observed.clone()));
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(250.0), px(100.0)),
            |_, _| view.clone(),
        );
        {
            let observations = observed.borrow();
            assert!(observations.created.len() <= 8);
            assert!(observations.created.iter().all(|index| *index < 8));
            assert_eq!(observations.laid_out[1].1.origin.x, px(110.0));
        }
        assert_eq!(handle.bounds().size.height, px(40.0));
        assert_eq!(handle.max_offset().width, px(109_740.0));

        *observed.borrow_mut() = Observations::default();
        handle.set_offset(point(px(-80_410.0), px(0.0))); // Card 731.
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(250.0), px(100.0)),
            |_, _| view.clone(),
        );
        {
            let observations = observed.borrow();
            assert!(observations.created.len() <= 8);
            assert_eq!(observations.created[0], 0, "one representative measurement");
            assert!(observations.laid_out.iter().all(|(index, _)| *index >= 729));
            assert!(
                observations
                    .laid_out
                    .iter()
                    .any(|(index, bounds)| *index == 731 && bounds.origin.x == px(0.0))
            );
        }
        cx.simulate_click(point(px(10.0), px(10.0)), Default::default());
        assert_eq!(observed.borrow().clicked, vec![731]);
    }

    #[gpui::test]
    fn final_card_has_no_trailing_gap_and_resize_remeasures(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let observed = Rc::new(RefCell::new(Observations::default()));
        let (view, cx) =
            cx.add_window_view(|_, _| Carousel::large(handle.clone(), observed.clone()));
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(250.0), px(100.0)),
            |_, _| view.clone(),
        );
        handle.set_offset(point(-handle.max_offset().width, px(0.0)));
        *observed.borrow_mut() = Observations::default();
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(250.0), px(100.0)),
            |_, _| view.clone(),
        );
        {
            let observations = observed.borrow();
            assert!(observations.created.len() <= 8);
            let (_, last) = observations.laid_out.last().unwrap();
            assert_eq!(observations.laid_out.last().unwrap().0, 999);
            assert_eq!(last.right(), px(250.0));
            assert_eq!(last.size.width, px(100.0));
        }

        view.update(cx, |this, _| {
            this.width = 650.0;
            this.card_width = 140.0;
            this.card_height = 80.0;
            this.measurement_index = 999;
        });
        *observed.borrow_mut() = Observations::default();
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(650.0), px(100.0)),
            |_, _| view.clone(),
        );
        assert_eq!(handle.bounds().size.height, px(80.0));
        assert_eq!(handle.max_offset().width, px(149_340.0));
        assert!(observed.borrow().created.len() <= 11);
        handle.set_offset(point(-handle.max_offset().width, px(0.0)));
        *observed.borrow_mut() = Observations::default();
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(650.0), px(100.0)),
            |_, _| view.clone(),
        );
        let observations = observed.borrow();
        let (index, last) = observations.laid_out.last().unwrap();
        assert_eq!(*index, 999);
        assert_eq!(last.right(), px(650.0));
        assert_eq!(last.size, size(px(140.0), px(80.0)));
        assert!(observations.created.len() <= 10);
    }

    #[gpui::test]
    fn respects_ancestor_clip_and_empty_or_shrinking_data(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let observed = Rc::new(RefCell::new(Observations::default()));
        let (view, cx) = cx.add_window_view(|_, _| {
            let mut carousel = Carousel::large(handle.clone(), observed.clone());
            carousel.width = 10_000.0;
            carousel.clip_width = Some(250.0);
            carousel.measurement_index = 999;
            carousel
        });
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(250.0), px(100.0)),
            |_, _| view.clone(),
        );
        assert!(observed.borrow().created.len() <= 6);
        assert!(
            observed
                .borrow()
                .laid_out
                .iter()
                .all(|(index, _)| *index < 8)
        );

        view.update(cx, |this, _| {
            this.width = 250.0;
            this.clip_width = None;
        });
        handle.set_offset(point(-handle.max_offset().width, px(0.0)));
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(250.0), px(100.0)),
            |_, _| view.clone(),
        );
        view.update(cx, |this, _| this.count = 2);
        *observed.borrow_mut() = Observations::default();
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(250.0), px(100.0)),
            |_, _| view.clone(),
        );
        assert_eq!(handle.offset().x, px(0.0));
        assert_eq!(handle.max_offset().width, px(0.0));
        assert_eq!(observed.borrow().laid_out.len(), 2);
        assert!(observed.borrow().created.iter().all(|index| *index < 2));

        view.update(cx, |this, _| this.count = 0);
        *observed.borrow_mut() = Observations::default();
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(250.0), px(100.0)),
            |_, _| view.clone(),
        );
        assert!(observed.borrow().created.is_empty());
        assert!(observed.borrow().laid_out.is_empty());
        assert_eq!(handle.max_offset().width, px(0.0));
        assert_eq!(handle.bounds().size.height, px(0.0));
    }
}
