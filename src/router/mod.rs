//! Router modules for navigation.
//!
//! Provides two types of routers:
//! - PathRouter: URL-like path-based routing (similar to React Router)
//! - StackRouter: Stack-based navigation (similar to React Navigation's Stack)

mod path_router;
mod stack_router;

pub use path_router::{PathRouter, PathRouterHandle, Route};
pub use stack_router::{StackRouter, StackRouterHandle};
