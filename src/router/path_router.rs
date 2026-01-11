//! Path-based router for top-level navigation.
//!
//! Similar to React Router, this provides URL-like path-based routing
//! for navigating between major sections of the app (e.g., sign-in vs main layout).

use gpui::{
    AnyView, App, Context, Empty, Entity, IntoElement, ParentElement, Render, Window, div,
    prelude::*,
};
use std::collections::HashMap;

/// A route definition with a path and a view factory.
pub struct Route {
    /// The path for this route (e.g., "/sign-in", "/main").
    pub path: String,
    /// Factory function to create the view for this route.
    factory: Box<dyn Fn(&mut Window, &mut Context<PathRouter>) -> AnyView + Send + Sync>,
}

impl Route {
    /// Creates a new route with the given path and view factory.
    ///
    /// The factory receives the window and a context, and should return a view
    /// that will be wrapped as an Entity automatically.
    pub fn new<F>(path: impl Into<String>, factory: F) -> Self
    where
        F: Fn(&mut Window, &mut Context<PathRouter>) -> AnyView + Send + Sync + 'static,
    {
        Self {
            path: path.into(),
            factory: Box::new(factory),
        }
    }
}

/// Path-based router component.
///
/// Usage (similar to React Router):
/// ```ignore
/// let router = cx.new(|cx| {
///     PathRouter::new()
///         .route(Route::new("/sign-in", |_, _| SignInScreen::new()))
///         .route(Route::new("/main", |_, cx| MainLayout::new(cx)))
///         .initial_path("/sign-in")
/// });
///
/// // Navigate to a different path
/// router.update(cx, |router, cx| {
///     router.navigate("/main", cx);
/// });
/// ```
pub struct PathRouter {
    routes: HashMap<String, Route>,
    current_path: String,
    current_view: Option<AnyView>,
}

impl PathRouter {
    /// Creates a new PathRouter with no routes.
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            current_path: String::new(),
            current_view: None,
        }
    }

    /// Adds a route to the router. Chainable.
    pub fn route(mut self, route: Route) -> Self {
        self.routes.insert(route.path.clone(), route);
        self
    }

    /// Sets the initial path. Must be called after routes are added.
    pub fn initial_path(mut self, path: impl Into<String>) -> Self {
        self.current_path = path.into();
        self
    }

    /// Navigates to a new path.
    /// Similar to React Router's navigate() or history.push().
    pub fn navigate(
        &mut self,
        path: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = path.into();
        if self.current_path != path {
            self.current_path = path.clone();
            self.current_view = self.create_view_for_path(&path, window, cx);
            cx.notify();
        }
    }

    /// Gets the current path.
    pub fn current_path(&self) -> &str {
        &self.current_path
    }

    fn create_view_for_path(
        &self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyView> {
        self.routes
            .get(path)
            .map(|route| (route.factory)(window, cx))
    }

    fn ensure_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_view.is_none() && !self.current_path.is_empty() {
            self.current_view = self.create_view_for_path(&self.current_path.clone(), window, cx);
        }
    }
}

impl Default for PathRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for PathRouter {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_view(window, cx);

        div().size_full().child(
            self.current_view
                .clone()
                .map(|v: AnyView| v.into_any_element())
                .unwrap_or_else(|| Empty.into_any_element()),
        )
    }
}

/// Global accessor for the path router.
/// Allows navigation from anywhere in the app.
#[derive(Clone)]
pub struct PathRouterHandle {
    router: Entity<PathRouter>,
}

impl gpui::Global for PathRouterHandle {}

impl PathRouterHandle {
    pub fn new(router: Entity<PathRouter>) -> Self {
        Self { router }
    }

    /// Navigates to a new path from anywhere in the app.
    /// Similar to React Router's useNavigate() hook.
    pub fn navigate(&self, path: impl Into<String>, window: &mut Window, cx: &mut App) {
        let path = path.into();
        self.router.update(cx, |router, cx| {
            router.navigate(path, window, cx);
        });
    }

    /// Gets the current path.
    pub fn current_path(&self, cx: &App) -> String {
        self.router.read(cx).current_path().to_string()
    }
}
