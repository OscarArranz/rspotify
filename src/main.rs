mod hooks;
mod lib;
mod router;
mod state;
mod view;

use anyhow::Result;
use gpui::{
    App, AppContext, Application, AssetSource, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};
use gpui_component::Root;
use reqwest_client::ReqwestClient;
use state::AppState;
use std::sync::Arc;
use view::root_layout::RootLayout;

struct Assets {}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        std::fs::read(path)
            .map(Into::into)
            .map_err(Into::into)
            .map(Some)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(std::fs::read_dir(path)?
            .filter_map(|entry| {
                Some(SharedString::from(
                    entry.ok()?.path().to_string_lossy().into_owned(),
                ))
            })
            .collect::<Vec<_>>())
    }
}

fn main() {
    // Load environment variables early
    dotenvy::dotenv().ok();

    Application::new()
        .with_assets(Assets {})
        .run(|cx: &mut App| {
            // Initialize global app state (similar to React Context Provider)
            cx.set_global(AppState::new());

            // Initialize HTTP client
            let http_client = ReqwestClient::user_agent("gpui example").unwrap();
            cx.set_http_client(Arc::new(http_client));

            // Initialize GPUI component
            gpui_component::init(cx);

            let bounds = Bounds::centered(None, size(px(1400.), px(800.0)), cx);

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
                |window, cx| {
                    let root_layout = cx.new(|cx| RootLayout::new(window, cx));
                    cx.new(|cx| Root::new(root_layout, window, cx))
                },
            )
            .unwrap();
        });
}
