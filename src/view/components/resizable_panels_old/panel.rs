//! Individual resizable panel component.

use gpui::{AnyElement, IntoElement};

/// Configuration for a resizable panel within a `ResizablePanelGroup`.
///
/// Each panel can specify:
/// - `min_size`: Minimum width in pixels (default: 0)
/// - `max_size`: Maximum width in pixels (default: unlimited)
/// - `initial_ratio`: Initial size as a fraction of the container (0.0 to 1.0)
pub struct ResizablePanel {
    /// Minimum width in pixels
    pub(crate) min_size: f32,
    /// Maximum width in pixels (None = no maximum)
    pub(crate) max_size: Option<f32>,
    /// Initial size as a fraction of the total width (0.0 to 1.0)
    pub(crate) initial_ratio: f32,
    /// The child element to render inside the panel
    pub(crate) child: Option<AnyElement>,
}

impl ResizablePanel {
    /// Creates a new resizable panel with default settings.
    pub fn new() -> Self {
        Self {
            min_size: 0.0,
            max_size: None,
            initial_ratio: 0.0, // Will be calculated if not set
            child: None,
        }
    }

    /// Sets the minimum size in pixels.
    pub fn min_size(mut self, size: f32) -> Self {
        self.min_size = size;
        self
    }

    /// Sets the maximum size in pixels.
    pub fn max_size(mut self, size: f32) -> Self {
        self.max_size = Some(size);
        self
    }

    /// Sets the initial size as a ratio of the container width (0.0 to 1.0).
    ///
    /// If initial ratios don't sum to 1.0, they will be normalized.
    /// If not set, panels will be distributed equally.
    pub fn initial_ratio(mut self, ratio: f32) -> Self {
        self.initial_ratio = ratio.clamp(0.0, 1.0);
        self
    }

    /// Sets the child element to render inside the panel.
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(child.into_any_element());
        self
    }
}

impl Default for ResizablePanel {
    fn default() -> Self {
        Self::new()
    }
}
