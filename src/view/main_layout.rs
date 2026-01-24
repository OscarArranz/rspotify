use gpui::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px, rgb,
};

use crate::router::{StackRouter, StackRouterHandle};
use crate::view::components::resizable::{
    ResizablePanel, ResizablePanelGroup, h_resizable, resizable_panel,
};

const PANEL_MARGIN: f32 = 9.0;

/// A simple home screen to show inside the stack router.
pub struct HomeScreen;

impl Render for HomeScreen {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .h_full()
            .child(div().w(px(PANEL_MARGIN)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .bg(rgb(0x121212))
                    .rounded_lg()
                    .gap_4()
                    .size_full()
                    .justify_center()
                    .items_center()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Welcome to Spotify!"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(div().size_8().bg(gpui::red()))
                            .child(div().size_8().bg(rgb(0x1DB954))) // Spotify green
                            .child(div().size_8().bg(gpui::blue()))
                            .child(div().size_8().bg(gpui::yellow()))
                            .child(div().size_8().bg(gpui::black()))
                            .child(div().size_8().bg(gpui::white())),
                    ),
            )
    }
}

/// Main layout containing the stack router and navigation.
///
/// This is the primary view after login, containing:
/// - A header with logout button
/// - The stack router for in-app navigation
pub struct MainLayout {
    stack_router: Entity<StackRouter>,
}

impl MainLayout {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Create the stack router with HomeScreen as initial screen
        let home = cx.new(|_| HomeScreen);
        let stack_router = cx.new(|_| StackRouter::with_initial(home.into()));

        // Register stack router globally so child components can navigate
        let stack_handle = StackRouterHandle::new(stack_router.clone());
        cx.set_global(stack_handle);

        Self { stack_router }
    }
}

impl Render for MainLayout {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Main content area with stack router
        // Note: Header/titlebar is now handled by RootLayout

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x000000))
            .text_color(rgb(0xffffff))
            .px(px(PANEL_MARGIN))
            .pb(px(PANEL_MARGIN))
            .child(
                h_resizable("main-layout")
                    .child(
                        resizable_panel().child(
                            div()
                                .size_full()
                                .bg(rgb(0x121212))
                                .rounded_lg()
                                .p_4()
                                .child("Left Panel"),
                        ),
                    )
                    .child(resizable_panel().child(self.stack_router.clone()))
                    .child(
                        resizable_panel().child(
                            div()
                                .size_full()
                                .bg(rgb(0x121212))
                                .ml(px(PANEL_MARGIN))
                                .rounded_lg()
                                .p_4()
                                .child("Right Panel"),
                        ),
                    ),
            )
    }
}
