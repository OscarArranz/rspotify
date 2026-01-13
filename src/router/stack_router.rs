//! Stack-based navigation router.
//!
//! Similar to React Navigation's Stack Navigator, this provides push/pop
//! navigation within a section of the app.

use gpui::{
    AnyView, App, Context, Empty, Entity, IntoElement, ParentElement, Render, Window, div,
    prelude::*,
};

/// Stack-based navigation router.
///
/// Usage (similar to React Navigation's Stack):
/// ```ignore
/// // Create the stack router
/// let stack = cx.new(|_| StackRouter::new());
///
/// // Push a new screen
/// stack.update(cx, |router, cx| {
///     let screen = cx.new(|_| DetailScreen::new());
///     router.push(screen.into(), cx);
/// });
///
/// // Pop the current screen
/// stack.update(cx, |router, cx| {
///     router.pop(cx);
/// });
/// ```
pub struct StackRouter {
    screens: Vec<AnyView>,
    current_screen: usize,
}

impl StackRouter {
    /// Creates a new empty StackRouter.
    pub fn new() -> Self {
        Self {
            screens: Vec::new(),
            current_screen: 0,
        }
    }

    /// Creates a StackRouter with an initial screen.
    pub fn with_initial(initial: AnyView) -> Self {
        Self {
            screens: vec![initial],
            current_screen: 0,
        }
    }

    /// Pushes a new screen onto the stack.
    /// Similar to React Navigation's navigation.push().
    pub fn push(&mut self, screen: AnyView, cx: &mut Context<Self>) {
        if self.current_screen + 1 < self.screens.len() {
            self.screens.truncate(self.current_screen + 1);
        }

        self.screens.push(screen);

        if self.screens.len() > 0 {
            self.current_screen += 1;
        }

        cx.notify();
    }

    /// Goes back to the previous screen in the stack.
    /// Similar to React Navigation's navigation.pop() or navigation.goBack().
    pub fn go_back(&mut self, cx: &mut Context<Self>) -> bool {
        if self.screens.len() > 1 && self.current_screen > 0 {
            self.current_screen -= 1;
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Goes forward to the next screen in the stack.
    /// Similar to React Navigation's navigation.pop() or navigation.goBack().
    pub fn go_forward(&mut self, cx: &mut Context<Self>) -> bool {
        if self.screens.len() > 1 && self.current_screen < self.screens.len() - 1 {
            self.current_screen += 1;
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Replaces the current screen with a new one.
    /// Similar to React Navigation's navigation.replace().
    pub fn replace(&mut self, screen: AnyView, cx: &mut Context<Self>) {
        if !self.screens.is_empty() {
            self.screens.pop();
        }
        self.screens.push(screen);
        cx.notify();
    }

    /// Pops all screens and pushes a new root screen.
    /// Similar to React Navigation's navigation.reset().
    pub fn reset(&mut self, screen: AnyView, cx: &mut Context<Self>) {
        self.screens.clear();
        self.screens.push(screen);
        cx.notify();
    }

    /// Returns the number of screens in the stack.
    pub fn depth(&self) -> usize {
        self.screens.len()
    }

    /// Returns true if we can go back (more than one screen).
    pub fn can_go_back(&self) -> bool {
        self.current_screen > 0
    }

    /// Returns true if we can go forward (more than one screen).
    pub fn can_go_forward(&self) -> bool {
        self.current_screen < self.screens.len() - 1
    }
}

impl Default for StackRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for StackRouter {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let current = self
            .screens
            .get(self.current_screen)
            .cloned()
            .map(|v: AnyView| v.into_any_element())
            .unwrap_or_else(|| Empty.into_any_element());

        div().size_full().child(current)
    }
}

/// Handle for accessing the StackRouter from child components.
///
/// Usage (similar to React Navigation's useNavigation hook):
/// ```ignore
/// // Get the handle from global state
/// let nav = cx.global::<StackRouterHandle>();
///
/// // Push a new screen
/// nav.push(detail_view.into(), cx);
///
/// // Go back
/// nav.pop(cx);
/// ```
#[derive(Clone)]
pub struct StackRouterHandle {
    router: Entity<StackRouter>,
}

impl gpui::Global for StackRouterHandle {}

impl StackRouterHandle {
    pub fn new(router: Entity<StackRouter>) -> Self {
        Self { router }
    }

    /// Pushes a new screen onto the stack.
    pub fn push(&self, screen: AnyView, cx: &mut App) {
        self.router.update(cx, |router, cx| {
            router.push(screen, cx);
        });
    }

    /// Pops the current screen.
    pub fn go_back(&self, cx: &mut App) -> bool {
        self.router.update(cx, |router, cx| router.go_back(cx))
    }

    /// Goes forward in the navigation history.
    pub fn go_forward(&self, cx: &mut App) -> bool {
        self.router.update(cx, |router, cx| router.go_forward(cx))
    }

    /// Replaces the current screen.
    pub fn replace(&self, screen: AnyView, cx: &mut App) {
        self.router.update(cx, |router, cx| {
            router.replace(screen, cx);
        });
    }

    /// Resets to a new root screen.
    pub fn reset(&self, screen: AnyView, cx: &mut App) {
        self.router.update(cx, |router, cx| {
            router.reset(screen, cx);
        });
    }

    /// Returns the stack depth.
    pub fn depth(&self, cx: &App) -> usize {
        self.router.read(cx).depth()
    }

    /// Returns true if navigation can go back.
    pub fn can_go_back(&self, cx: &App) -> bool {
        self.router.read(cx).can_go_back()
    }

    /// Returns true if navigation can go forward.
    pub fn can_go_forward(&self, cx: &App) -> bool {
        self.router.read(cx).can_go_forward()
    }

    /// Gets the underlying Entity for direct access.
    pub fn entity(&self) -> Entity<StackRouter> {
        self.router.clone()
    }
}
