//! Resizable panel group container component.

use gpui::prelude::FluentBuilder;
use gpui::{
    App, Bounds, Element, ElementId, Entity, GlobalElementId, InteractiveElement, IntoElement,
    LayoutId, MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, SharedString,
    Style, Styled, Window, canvas, div, px,
};

use super::panel::ResizablePanel;
use super::resize_handle::{ResizeHandleState, resize_handle};

const HANDLE_WIDTH: f32 = 6.0;

/// State for tracking drag operations.
#[derive(Clone)]
pub struct DragState {
    /// Index of the handle being dragged (between panel i and i+1)
    pub handle_index: usize,
    /// Starting X position of the drag
    pub start_x: f32,
    /// Panel sizes (in pixels) at the start of the drag
    pub start_sizes: Vec<f32>,
}

/// Internal state for the panel group.
#[derive(Clone)]
pub struct ResizablePanelGroupState {
    /// Current size of each panel in pixels
    pub sizes: Vec<f32>,
    /// Current drag operation if any
    pub drag_state: Option<DragState>,
    /// Container bounds
    pub bounds: Bounds<Pixels>,
    /// Whether sizes have been initialized
    pub initialized: bool,
}

impl Default for ResizablePanelGroupState {
    fn default() -> Self {
        Self {
            sizes: Vec::new(),
            drag_state: None,
            bounds: Bounds::default(),
            initialized: false,
        }
    }
}

/// Helper struct to store panel constraints for resize calculations.
#[derive(Clone)]
pub struct PanelConstraints {
    pub min_sizes: Vec<f32>,
    pub max_sizes: Vec<Option<f32>>,
    pub initial_ratios: Vec<f32>,
}

impl PanelConstraints {
    pub fn from_panels(panels: &[ResizablePanel]) -> Self {
        let panel_count = panels.len();
        let initial_ratios: Vec<f32> = panels.iter().map(|p| p.initial_ratio).collect();
        let total: f32 = initial_ratios.iter().sum();

        let normalized_ratios = if total > 0.0 {
            initial_ratios.iter().map(|&r| r / total).collect()
        } else {
            vec![1.0 / panel_count as f32; panel_count]
        };

        Self {
            min_sizes: panels.iter().map(|p| p.min_size).collect(),
            max_sizes: panels.iter().map(|p| p.max_size).collect(),
            initial_ratios: normalized_ratios,
        }
    }

    pub fn clamp_size(&self, panel_index: usize, size: f32) -> f32 {
        let min = self.min_sizes.get(panel_index).copied().unwrap_or(0.0);
        let max = self
            .max_sizes
            .get(panel_index)
            .and_then(|m| *m)
            .unwrap_or(f32::MAX);
        size.clamp(min, max)
    }

    /// Calculate initial sizes based on available width and ratios
    pub fn calculate_initial_sizes(&self, available_width: f32) -> Vec<f32> {
        self.initial_ratios
            .iter()
            .enumerate()
            .map(|(i, &ratio)| self.clamp_size(i, ratio * available_width))
            .collect()
    }

    pub fn handle_resize(
        &self,
        handle_index: usize,
        delta_x: f32,
        start_sizes: &[f32],
    ) -> Vec<f32> {
        let mut new_sizes = start_sizes.to_vec();

        let left_index = handle_index;
        let right_index = handle_index + 1;

        if right_index >= start_sizes.len() {
            return new_sizes;
        }

        // Calculate new sizes for the two affected panels
        let new_left_size = start_sizes[left_index] + delta_x;
        let new_right_size = start_sizes[right_index] - delta_x;

        // Clamp to min/max constraints
        let clamped_left = self.clamp_size(left_index, new_left_size);
        let clamped_right = self.clamp_size(right_index, new_right_size);

        // Calculate the actual achievable delta
        let left_delta = clamped_left - start_sizes[left_index];
        let right_delta = start_sizes[right_index] - clamped_right;

        // Use the smaller of the two deltas to ensure balance
        let balanced_delta = if delta_x >= 0.0 {
            left_delta.min(right_delta)
        } else {
            -((-left_delta).min(-right_delta))
        };

        // Apply the balanced resize
        new_sizes[left_index] = start_sizes[left_index] + balanced_delta;
        new_sizes[right_index] = start_sizes[right_index] - balanced_delta;

        new_sizes
    }
}

/// A container that manages multiple horizontally resizable panels.
pub struct ResizablePanelGroup {
    id: SharedString,
    panels: Vec<ResizablePanel>,
}

impl ResizablePanelGroup {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            panels: Vec::new(),
        }
    }

    pub fn child(mut self, panel: ResizablePanel) -> Self {
        self.panels.push(panel);
        self
    }
}

impl IntoElement for ResizablePanelGroup {
    type Element = gpui::Component<Self>;

    fn into_element(self) -> Self::Element {
        gpui::Component::new(self)
    }
}

impl gpui::RenderOnce for ResizablePanelGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let panel_count = self.panels.len();
        let constraints = PanelConstraints::from_panels(&self.panels);

        // Create state for the panel group
        let state: Entity<ResizablePanelGroupState> = window.use_keyed_state(
            SharedString::from(format!("resizable-state-{}", self.id)),
            cx,
            |_window, _cx| ResizablePanelGroupState::default(),
        );

        // Create handle states
        let handle_states: Vec<Entity<ResizeHandleState>> = (0..panel_count.saturating_sub(1))
            .map(|i| {
                window.use_keyed_state(
                    SharedString::from(format!("{}-handle-state-{}", self.id, i)),
                    cx,
                    |_window, _cx| ResizeHandleState::default(),
                )
            })
            .collect();

        let sizes = state.read(cx).sizes.clone();
        let initialized = state.read(cx).initialized;

        // Build children: panels interspersed with handles
        let mut children: Vec<gpui::AnyElement> = Vec::new();

        for (i, panel) in self.panels.into_iter().enumerate() {
            let size = sizes.get(i).copied();
            let panel_child = panel.child;

            // Panel uses flex_basis for size when initialized, otherwise flex_grow for initial layout
            let panel_element = div()
                .id(SharedString::from(format!("{}-panel-{}", self.id, i)))
                .h_full()
                .overflow_hidden()
                .min_w(px(panel.min_size))
                .when_some(panel.max_size, |div, max| div.max_w(px(max)))
                .when_some(panel_child, |div, child| div.child(child))
                .when(initialized && size.is_some(), |div| {
                    div.flex_shrink_0().flex_basis(px(size.unwrap()))
                })
                .when(!initialized || size.is_none(), |div| div.flex_1());

            children.push(panel_element.into_any_element());

            // Add resize handle after each panel except the last
            if i < panel_count - 1 {
                if let Some(handle_state) = handle_states.get(i).cloned() {
                    let state_for_start = state.clone();
                    let state_for_end = state.clone();
                    let handle_index = i;

                    let handle = resize_handle(
                        SharedString::from(format!("{}-handle-{}", self.id, i)),
                        handle_state,
                        // On drag start
                        move |x, _window, cx| {
                            let sizes = state_for_start.read(cx).sizes.clone();
                            state_for_start.update(cx, |s, cx| {
                                s.drag_state = Some(DragState {
                                    handle_index,
                                    start_x: x,
                                    start_sizes: sizes,
                                });
                                cx.notify();
                            });
                        },
                        // On drag end
                        move |_window, cx| {
                            state_for_end.update(cx, |s, cx| {
                                s.drag_state = None;
                                cx.notify();
                            });
                        },
                        cx,
                    );

                    children.push(handle.into_any_element());
                }
            }
        }

        // Create the main container with mouse event handling and bounds tracking
        let state_for_canvas = state.clone();
        let constraints_for_canvas = constraints.clone();
        let state_for_move = state.clone();
        let state_for_up = state.clone();
        let constraints_for_move = constraints.clone();

        div()
            .id(SharedString::from(format!("resizable-group-{}", self.id)))
            .flex()
            .flex_row()
            .size_full()
            .overflow_hidden()
            // Canvas to track container bounds and initialize sizes
            .child(
                canvas(
                    move |bounds, _window, cx| {
                        state_for_canvas.update(cx, |state, cx| {
                            let container_width: f32 = bounds.size.width.into();
                            let handle_count = panel_count.saturating_sub(1);
                            let total_handle_width = handle_count as f32 * HANDLE_WIDTH;
                            let available_width = (container_width - total_handle_width).max(0.0);

                            // Initialize sizes on first render or when container size changes significantly
                            if !state.initialized || state.sizes.len() != panel_count {
                                state.sizes =
                                    constraints_for_canvas.calculate_initial_sizes(available_width);
                                state.initialized = true;
                                state.bounds = bounds;
                                cx.notify();
                            } else if (f32::from(state.bounds.size.width)
                                - f32::from(bounds.size.width))
                            .abs()
                                > 1.0
                            {
                                // Container resized - scale all panel sizes proportionally
                                let old_width: f32 = state.bounds.size.width.into();
                                let old_handle_width = handle_count as f32 * HANDLE_WIDTH;
                                let old_available = (old_width - old_handle_width).max(1.0);

                                let scale = available_width / old_available;
                                state.sizes = state
                                    .sizes
                                    .iter()
                                    .enumerate()
                                    .map(|(i, &s)| constraints_for_canvas.clamp_size(i, s * scale))
                                    .collect();
                                state.bounds = bounds;
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
                let drag_state = state_for_move.read(cx).drag_state.clone();
                if let Some(ds) = drag_state {
                    let current_x: f32 = event.position.x.into();
                    let delta_x = current_x - ds.start_x;

                    let new_sizes = constraints_for_move.handle_resize(
                        ds.handle_index,
                        delta_x,
                        &ds.start_sizes,
                    );

                    state_for_move.update(cx, |s, cx| {
                        s.sizes = new_sizes;
                        cx.notify();
                    });
                }
            })
            .on_mouse_up(MouseButton::Left, move |_event, _window, cx| {
                let is_dragging = state_for_up.read(cx).drag_state.is_some();
                if is_dragging {
                    state_for_up.update(cx, |s, cx| {
                        s.drag_state = None;
                        cx.notify();
                    });
                }
            })
            .children(children)
    }
}
