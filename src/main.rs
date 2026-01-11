mod hooks;
mod lib;
mod router;
mod state;
mod view;

use gpui::{
    App, Application, Bounds, Entity, IntoElement, ParentElement, Render, TitlebarOptions, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, size,
};

use router::{PathRouter, PathRouterHandle, Route};
use state::AppState;
use view::main_layout::MainLayout;
use view::screens::sign_in::SignInScreen;

/// Root application component that holds the path router.
struct RootApp {
    router: Entity<PathRouter>,
}

impl RootApp {
    fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // Create the path router with routes
        let router = cx.new(|_| {
            PathRouter::new()
                .route(Route::new("/sign-in", |_window, cx| {
                    cx.new(|_| SignInScreen::new()).into()
                }))
                .route(Route::new("/main", |window, cx| {
                    cx.new(|cx| MainLayout::new(window, cx)).into()
                }))
                .initial_path("/sign-in")
        });

        // Register path router globally so components can navigate
        let router_handle = PathRouterHandle::new(router.clone());
        cx.set_global(router_handle);

        Self { router }
    }
}

impl Render for RootApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().child(self.router.clone())
    }
}

fn main() {
    // Load environment variables early
    dotenvy::dotenv().ok();

    Application::new().run(|cx: &mut App| {
        // Initialize global app state (similar to React Context Provider)
        cx.set_global(AppState::new());

        let bounds = Bounds::centered(None, size(px(500.), px(600.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: true,
                    title: Some("Spotify".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| RootApp::new(window, cx)),
        )
        .unwrap();
    });
}
