use std::ops::Range;

use gpui::{
    Along, App, Axis, Bounds, Context, ElementId, EventEmitter, IsZero, Pixels, Window, px,
};

mod panel;
mod resize_handle;
pub use panel::*;
pub(crate) use resize_handle::*;
mod element_ext;

/// A trait for converting [`Pixels`] to `f32` and `f64`.
pub trait PixelsExt {
    fn as_f32(&self) -> f32;
    fn as_f64(self) -> f64;
}
impl PixelsExt for Pixels {
    fn as_f32(&self) -> f32 {
        f32::from(self)
    }

    fn as_f64(self) -> f64 {
        f64::from(self)
    }
}

pub(crate) const PANEL_MIN_SIZE: Pixels = px(100.);

/// Create a [`ResizablePanelGroup`] with horizontal resizing
pub fn h_resizable(id: impl Into<ElementId>) -> ResizablePanelGroup {
    ResizablePanelGroup::new(id).axis(Axis::Horizontal)
}

/// Create a [`ResizablePanelGroup`] with vertical resizing
pub fn v_resizable(id: impl Into<ElementId>) -> ResizablePanelGroup {
    ResizablePanelGroup::new(id).axis(Axis::Vertical)
}

/// Create a [`ResizablePanel`].
pub fn resizable_panel() -> ResizablePanel {
    ResizablePanel::new()
}

/// State for a [`ResizablePanel`]
#[derive(Debug, Clone)]
pub struct ResizableState {
    /// The `axis` will sync to actual axis of the ResizablePanelGroup in use.
    axis: Axis,
    panels: Vec<ResizablePanelState>,
    sizes: Vec<Pixels>,
    pub(crate) resizing_panel_ix: Option<usize>,
    bounds: Bounds<Pixels>,
}

impl Default for ResizableState {
    fn default() -> Self {
        Self {
            axis: Axis::Horizontal,
            panels: vec![],
            sizes: vec![],
            resizing_panel_ix: None,
            bounds: Bounds::default(),
        }
    }
}

impl ResizableState {
    /// Set the library/content split without synthesizing a mouse drag.
    pub(crate) fn set_first_size(&mut self, size: Pixels, cx: &mut Context<Self>) {
        if self.panels.len() != 2 || self.container_size() <= px(0.0) {
            return;
        }
        let total = self.container_size();
        let size = size.clamp(px(80.0), (total - px(240.0)).max(px(80.0)));
        self.sizes = vec![size, total - size];
        for (panel, size) in self.panels.iter_mut().zip(&self.sizes) {
            panel.size = Some(*size);
        }
        cx.notify();
    }

    /// Get the size of the panels.
    pub fn sizes(&self) -> &Vec<Pixels> {
        &self.sizes
    }

    pub(crate) fn insert_panel(
        &mut self,
        size: Option<Pixels>,
        ix: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let panel_state = ResizablePanelState {
            size,
            ..Default::default()
        };

        let size = size.unwrap_or(PANEL_MIN_SIZE);

        // We make sure that the size always sums up to the container size
        // by reducing the size of all other panels first.
        let container_size = self.container_size().max(px(1.));
        let total_leftover_size = (container_size - size).max(px(1.));

        for (i, panel) in self.panels.iter_mut().enumerate() {
            let ratio = self.sizes[i] / container_size;
            self.sizes[i] = total_leftover_size * ratio;
            panel.size = Some(self.sizes[i]);
        }

        if let Some(ix) = ix {
            self.panels.insert(ix, panel_state);
            self.sizes.insert(ix, size);
        } else {
            self.panels.push(panel_state);
            self.sizes.push(size);
        };

        cx.notify();
    }

    pub(crate) fn sync_panels_count(
        &mut self,
        axis: Axis,
        panels_count: usize,
        cx: &mut Context<Self>,
    ) {
        let mut changed = self.axis != axis;
        self.axis = axis;

        if panels_count > self.panels.len() {
            let diff = panels_count - self.panels.len();
            self.panels
                .extend(vec![ResizablePanelState::default(); diff]);
            self.sizes.extend(vec![PANEL_MIN_SIZE; diff]);
            changed = true;
        }

        if panels_count < self.panels.len() {
            self.panels.truncate(panels_count);
            self.sizes.truncate(panels_count);
            changed = true;
        }

        if changed {
            // We need to make sure the total size is in line with the container size.
            self.adjust_to_container_size(cx);
        }
    }

    pub(crate) fn update_panel_size(
        &mut self,
        panel_ix: usize,
        bounds: Bounds<Pixels>,
        size_range: Range<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let size = bounds.size.along(self.axis);
        // Keep measured geometry authoritative after mode changes and window resizes.
        let changed = self.sizes[panel_ix] != size
            || self.panels[panel_ix].bounds != bounds
            || self.panels[panel_ix].size_range != size_range;
        self.sizes[panel_ix] = size;
        self.panels[panel_ix].size = Some(size);
        self.panels[panel_ix].bounds = bounds;
        self.panels[panel_ix].size_range = size_range;
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn remove_panel(&mut self, panel_ix: usize, cx: &mut Context<Self>) {
        self.panels.remove(panel_ix);
        self.sizes.remove(panel_ix);
        if let Some(resizing_panel_ix) = self.resizing_panel_ix {
            if resizing_panel_ix > panel_ix {
                self.resizing_panel_ix = Some(resizing_panel_ix - 1);
            }
        }
        self.adjust_to_container_size(cx);
    }

    pub(crate) fn replace_panel(
        &mut self,
        panel_ix: usize,
        panel: ResizablePanelState,
        cx: &mut Context<Self>,
    ) {
        let old_size = self.sizes[panel_ix];

        self.panels[panel_ix] = panel;
        self.sizes[panel_ix] = old_size;
        self.adjust_to_container_size(cx);
    }

    pub(crate) fn clear(&mut self) {
        self.panels.clear();
        self.sizes.clear();
    }

    #[inline]
    pub(crate) fn container_size(&self) -> Pixels {
        self.bounds.size.along(self.axis)
    }

    pub(crate) fn done_resizing(&mut self, cx: &mut Context<Self>) {
        self.resizing_panel_ix = None;
        cx.emit(ResizablePanelEvent::Resized);
    }

    fn panel_size_range(&self, ix: usize) -> Range<Pixels> {
        let Some(panel) = self.panels.get(ix) else {
            return PANEL_MIN_SIZE..Pixels::MAX;
        };

        panel.size_range.clone()
    }

    fn sync_real_panel_sizes(&mut self, _: &App) {
        for (i, panel) in self.panels.iter().enumerate() {
            self.sizes[i] = panel.bounds.size.along(self.axis);
        }
    }

    /// The `ix`` is the index of the panel to resize,
    /// and the `size` is the new size for the panel.
    fn resize_panel(&mut self, ix: usize, size: Pixels, _: &mut Window, cx: &mut Context<Self>) {
        let old_sizes = self.sizes.clone();

        let mut ix = ix;
        // Only resize the left panels.
        if ix >= old_sizes.len() - 1 {
            return;
        }
        let container_size = self.container_size();
        self.sync_real_panel_sizes(cx);

        let move_changed = size - old_sizes[ix];
        if move_changed == px(0.) {
            return;
        }

        let size_range = self.panel_size_range(ix);
        let new_size = size.clamp(size_range.start, size_range.end);
        let is_expand = move_changed > px(0.);

        let main_ix = ix;
        let mut new_sizes = old_sizes.clone();

        if is_expand {
            let mut changed = new_size - old_sizes[ix];
            new_sizes[ix] = new_size;

            while changed > px(0.) && ix < old_sizes.len() - 1 {
                ix += 1;
                let size_range = self.panel_size_range(ix);
                let available_size = (new_sizes[ix] - size_range.start).max(px(0.));
                let to_reduce = changed.min(available_size);
                new_sizes[ix] -= to_reduce;
                changed -= to_reduce;
            }
        } else {
            let mut changed = new_size - size;
            new_sizes[ix] = new_size;

            while changed > px(0.) && ix > 0 {
                ix -= 1;
                let size_range = self.panel_size_range(ix);
                let available_size = (new_sizes[ix] - size_range.start).max(px(0.));
                let to_reduce = changed.min(available_size);
                changed -= to_reduce;
                new_sizes[ix] -= to_reduce;
            }

            new_sizes[main_ix + 1] += old_sizes[main_ix] - size - changed;
        }

        // If total size exceeds container size, adjust the main panel
        let total_size: Pixels = new_sizes.iter().map(|s| s.as_f32()).sum::<f32>().into();
        if total_size > container_size {
            let overflow = total_size - container_size;
            new_sizes[main_ix] = (new_sizes[main_ix] - overflow).max(size_range.start);
        }

        for (i, _) in old_sizes.iter().enumerate() {
            let size = new_sizes[i];
            self.panels[i].size = Some(size);
        }
        self.sizes = new_sizes;
        cx.notify();
    }

    /// Adjust panel sizes according to the container size.
    ///
    /// When the container size changes, the panels should take up the same percentage as they did before.
    fn adjust_to_container_size(&mut self, cx: &mut Context<Self>) {
        if self.container_size().is_zero() {
            return;
        }

        let container_size = self.container_size();
        let total_size = px(self.sizes.iter().map(|s| s.as_f32()).sum::<f32>());
        if total_size <= px(0.0) {
            return;
        }

        // The library's collapsed width must stay fixed when the window is resized.
        // Proportional scaling alone can leave a phantom gutter because rendering clamps
        // the first panel but the stored content width still uses the unclamped value.
        if self.panels.len() == 2 {
            let desired = container_size * (self.sizes[0] / total_size);
            if let Some(sizes) = fit_split(
                container_size,
                desired,
                self.panel_size_range(0),
                self.panel_size_range(1),
            ) {
                self.sizes = sizes.to_vec();
                for (panel, size) in self.panels.iter_mut().zip(sizes) {
                    panel.size = Some(size);
                }
                cx.notify();
                return;
            }
        }

        for i in 0..self.panels.len() {
            let size = self.sizes[i];
            let ratio = size / total_size;
            let new_size = container_size * ratio;

            self.sizes[i] = new_size;
            self.panels[i].size = Some(new_size);
        }
        cx.notify();
    }
}

fn fit_split(
    total: Pixels,
    desired: Pixels,
    first: Range<Pixels>,
    second: Range<Pixels>,
) -> Option<[Pixels; 2]> {
    // Before the first prepaint, panel constraints have not been populated yet.
    if first.end <= px(0.0) || second.end <= px(0.0) {
        return None;
    }
    let minimum = first.start.max(total - second.end);
    let maximum = first.end.min(total - second.start);
    if minimum > maximum {
        return None;
    }
    let first = desired.clamp(minimum, maximum);
    Some([first, total - first])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_library_stays_fixed_on_window_resize() {
        assert_eq!(
            fit_split(
                px(1200.0),
                px(150.0),
                px(80.0)..px(80.0),
                px(240.0)..Pixels::MAX
            ),
            Some([px(80.0), px(1120.0)])
        );
    }

    #[test]
    fn expanded_library_leaves_space_for_content() {
        assert_eq!(
            fit_split(
                px(700.0),
                px(593.0),
                px(240.0)..px(800.0),
                px(240.0)..Pixels::MAX
            ),
            Some([px(460.0), px(240.0)])
        );
        assert_eq!(
            fit_split(
                px(1400.0),
                px(900.0),
                px(240.0)..px(800.0),
                px(240.0)..Pixels::MAX
            ),
            Some([px(800.0), px(600.0)])
        );
    }

    #[test]
    fn impossible_or_uninitialized_constraints_do_not_panic() {
        assert_eq!(
            fit_split(
                px(300.0),
                px(240.0),
                px(240.0)..px(800.0),
                px(240.0)..Pixels::MAX
            ),
            None
        );
        assert_eq!(
            fit_split(px(900.0), px(300.0), px(0.0)..px(0.0), px(0.0)..px(0.0)),
            None
        );
    }
}

impl EventEmitter<ResizablePanelEvent> for ResizableState {}

#[derive(Debug, Clone, Default)]
pub(crate) struct ResizablePanelState {
    pub size: Option<Pixels>,
    pub size_range: Range<Pixels>,
    bounds: Bounds<Pixels>,
}
