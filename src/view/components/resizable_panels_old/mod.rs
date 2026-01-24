//! Resizable panels components for creating horizontally resizable layouts.
//!
//! This module provides components for building layouts with resizable panels:
//! - `ResizablePanelGroup`: Container that manages multiple resizable panels
//! - `ResizablePanel`: Individual panel with configurable min/max size constraints
//! - `resize_handle`: Draggable handle between panels for resizing
//!
//! # Example
//! ```rust
//! use crate::view::components::resizable_panels::{ResizablePanelGroup, ResizablePanel};
//!
//! ResizablePanelGroup::new("my-layout")
//!     .child(ResizablePanel::new().min_size(100.0).max_size(400.0).initial_ratio(0.25))
//!     .child(ResizablePanel::new().min_size(200.0).initial_ratio(0.5))
//!     .child(ResizablePanel::new().min_size(100.0).initial_ratio(0.25))
//! ```

mod panel;
mod panel_group;
mod resize_handle;

pub use panel::ResizablePanel;
pub use panel_group::ResizablePanelGroup;
