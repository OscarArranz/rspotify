//! React-like hooks for GPUI.
//!
//! These hooks provide familiar patterns for React developers:
//! - use_app_state: Similar to useContext for global app state
//! - use_navigate: Similar to React Router's useNavigate
//! - use_stack_nav: Similar to React Navigation's useNavigation
//!
//! Note: In GPUI, "hooks" are just helper functions that access context.
//! Unlike React hooks, they don't have the same rules about ordering.

use gpui::{App, AppContext, Window};

use crate::router::{PathRouterHandle, StackRouterHandle};
use crate::state::AppState;

/// Reads the global app state (similar to useContext(AppContext)).
///
/// Usage:
/// ```ignore
/// use_app_state(cx, |state, _app| {
///     if state.auth.is_authenticated() {
///         // user is logged in
///     }
/// });
/// ```
pub fn use_app_state<C: AppContext<Result<R> = R>, R>(
    cx: &C,
    f: impl FnOnce(&AppState, &App) -> R,
) -> R {
    cx.read_global::<AppState, R>(f)
}

/// Updates the global app state (similar to useContext + setState).
///
/// Usage:
/// ```ignore
/// use_set_app_state(cx, |state| {
///     state.set_auth(&token);
/// });
/// ```
pub fn use_set_app_state<F, R>(cx: &mut App, f: F) -> R
where
    F: FnOnce(&mut AppState) -> R,
{
    gpui::BorrowAppContext::update_global::<AppState, _>(cx, |state, _cx| f(state))
}

/// Gets the path router handle for navigation (similar to useNavigate).
///
/// Usage:
/// ```ignore
/// let navigate = use_navigate(cx);
/// navigate.navigate("/main", window, cx);
/// ```
pub fn use_navigate(cx: &App) -> PathRouterHandle {
    cx.global::<PathRouterHandle>().clone()
}

/// Gets the stack navigation handle (similar to useNavigation in React Navigation).
///
/// Usage:
/// ```ignore
/// let nav = use_stack_nav(cx);
/// nav.push(detail_screen.into(), cx);
/// nav.pop(cx);
/// ```
pub fn use_stack_nav(cx: &App) -> Option<StackRouterHandle> {
    cx.try_global::<StackRouterHandle>().cloned()
}

/// Navigates to a path (convenience function combining use_navigate).
///
/// Usage:
/// ```ignore
/// navigate_to("/main", window, cx);
/// ```
pub fn navigate_to(path: impl Into<String>, window: &mut Window, cx: &mut App) {
    let handle = cx.global::<PathRouterHandle>().clone();
    handle.navigate(path, window, cx);
}

/// Checks if the user is authenticated.
///
/// Usage:
/// ```ignore
/// if is_authenticated(cx) {
///     // show user content
/// }
/// ```
pub fn is_authenticated(cx: &App) -> bool {
    cx.try_global::<AppState>()
        .map(|s| s.auth.is_authenticated())
        .unwrap_or(false)
}
