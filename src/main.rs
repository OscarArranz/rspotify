mod hooks;
mod lib;
mod router;
mod state;
mod view;

use anyhow::{Context, Result};
use gpui::{
    App, AppContext, Application, AssetSource, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};
use gpui_component::Root;
use reqwest_client::ReqwestClient;
use rust_embed::RustEmbed;
use state::AppState;
use std::sync::Arc;
use view::root_layout::RootLayout;

#[derive(RustEmbed)]
#[folder = "./assets"]
#[include = "fonts/**/*"]
#[include = "icons/**/*"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        Self::get(path)
            .map(|f| Some(f.data))
            .with_context(|| format!("loading asset at path {path:?}"))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| {
                if p.starts_with(path) {
                    Some(p.into())
                } else {
                    None
                }
            })
            .collect())
    }
}

impl Assets {
    /// Populate the [`TextSystem`] of the given [`AppContext`] with all `.ttf` fonts in the `fonts` directory.
    pub fn load_fonts(&self, cx: &App) -> anyhow::Result<()> {
        let font_paths = self.list("fonts")?;
        let mut embedded_fonts = Vec::new();
        for font_path in font_paths {
            if font_path.ends_with(".ttf") {
                println!("Loading font: {}", font_path);
                let font_bytes = cx
                    .asset_source()
                    .load(&font_path)?
                    .expect("Assets should never return None");
                embedded_fonts.push(font_bytes);
            }
        }

        cx.text_system().add_fonts(embedded_fonts)
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

            // Load embedded fonts
            Assets.load_fonts(cx).unwrap();

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
