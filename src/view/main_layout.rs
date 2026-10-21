use std::time::Duration;

use futures::channel::oneshot;
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, img, px, relative, rgb, svg,
};

use crate::lib::spotify::{LibraryPlaylist, NativeSpotify, PlaybackSnapshot, PlaylistSource};
use crate::router::{StackRouter, StackRouterHandle};
use crate::state::AppState;
use crate::view::components::resizable::{h_resizable, resizable_panel};
use crate::view::screens::playlist::{PlaylistScreen, render_cover};

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
                            .text_color(rgb(0xb3b3b3))
                            .child("Search for music and press play. Playback requires Premium."),
                    ),
            )
    }
}

/// Main layout containing the stack router, library and player bar.
pub struct MainLayout {
    stack_router: Entity<StackRouter>,
    library: Entity<LibrarySidebar>,
    player: Option<Entity<PlayerBar>>,
}

impl MainLayout {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Create the stack router with HomeScreen as initial screen
        let home = cx.new(|_| HomeScreen);
        let stack_router = cx.new(|_| StackRouter::with_initial(home.into()));

        // Register stack router globally so child components can navigate
        let stack_handle = StackRouterHandle::new(stack_router.clone());
        cx.set_global(stack_handle);

        let client = cx
            .try_global::<AppState>()
            .and_then(AppState::spotify_client);
        let library = cx.new(|cx| LibrarySidebar::new(client.clone(), cx));
        let player = client.map(|client| cx.new(|cx| PlayerBar::new(client, cx)));

        Self {
            stack_router,
            library,
            player,
        }
    }
}

impl Render for MainLayout {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x000000))
            .text_color(rgb(0xffffff))
            .px(px(PANEL_MARGIN))
            .pb(px(PANEL_MARGIN))
            .child(
                div().flex_1().overflow_hidden().child(
                    h_resizable("main-layout")
                        .child(
                            resizable_panel()
                                .size(px(340.0))
                                .child(self.library.clone()),
                        )
                        .child(resizable_panel().child(self.stack_router.clone())),
                ),
            )
            .children(self.player.clone())
    }
}

struct LibrarySidebar {
    entries: Vec<LibraryPlaylist>,
    client: Option<NativeSpotify>,
}

impl LibrarySidebar {
    fn new(client: Option<NativeSpotify>, cx: &mut Context<Self>) -> Self {
        let sidebar = Self {
            entries: Vec::new(),
            client: client.clone(),
        };
        if let Some(client) = client {
            cx.spawn(async move |this, cx| {
                let (tx, rx) = oneshot::channel();
                std::thread::spawn(move || {
                    let result = client.library_playlists();
                    let _ = tx.send(result);
                });

                if let Ok(Ok(entries)) = rx.await {
                    let _ = this.update(cx, |this, cx| {
                        this.entries = entries;
                        cx.notify();
                    });
                }
            })
            .detach();
        }
        sidebar
    }
}

impl Render for LibrarySidebar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let liked_client = self.client.clone();
        let entries = self.entries.iter().enumerate().map(|(index, entry)| {
            let client = self.client.clone();
            let source = PlaylistSource::Playlist {
                uri: entry.uri.clone(),
                name: entry.name.clone(),
                owner: entry.owner.clone(),
                image_url: entry.image_url.clone(),
            };
            div()
                .id(("library-entry", index))
                .flex()
                .items_center()
                .gap(px(10.0))
                .p(px(6.0))
                .rounded(px(5.0))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0x242424)))
                .on_click(move |_, _, cx| {
                    let Some(client) = &client else {
                        return;
                    };
                    let Some(router) = cx.try_global::<StackRouterHandle>().cloned() else {
                        return;
                    };
                    let screen =
                        cx.new(|cx| PlaylistScreen::new(source.clone(), client.clone(), cx));
                    router.push(screen.into(), cx);
                })
                .child(render_cover(
                    entry.image_url.as_deref(),
                    40.0,
                    false,
                    0x3a2d64,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(entry.name.clone())
                        .child(div().text_sm().text_color(rgb(0xb3b3b3)).child(
                            if entry.owner.is_empty() {
                                "Playlist".to_string()
                            } else {
                                format!("Playlist · {}", entry.owner)
                            },
                        )),
                )
        });

        div()
            .flex()
            .flex_col()
            .size_full()
            .mr(px(PANEL_MARGIN))
            .p(px(16.0))
            .gap(px(18.0))
            .bg(rgb(0x121212))
            .rounded_lg()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_color(rgb(0xb3b3b3))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Your Library"),
                    )
                    .child(
                        div()
                            .px(px(12.0))
                            .py(px(7.0))
                            .rounded(px(18.0))
                            .bg(rgb(0x242424))
                            .text_sm()
                            .child("＋ Create"),
                    ),
            )
            .child(div().flex().flex_wrap().gap(px(7.0)).children(
                ["Playlists", "Podcasts", "Albums", "Artists"].map(|filter| {
                    div()
                        .px(px(11.0))
                        .py(px(7.0))
                        .rounded(px(16.0))
                        .bg(rgb(0x2a2a2a))
                        .text_sm()
                        .child(filter)
                }),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_color(rgb(0xb3b3b3))
                    .text_sm()
                    .child("⌕")
                    .child("Recents  ☰"),
            )
            .child(
                div()
                    .id("liked-songs")
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .p(px(6.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0x242424)))
                    .on_click(move |_, _, cx| {
                        let Some(client) = &liked_client else {
                            return;
                        };
                        let Some(router) = cx.try_global::<StackRouterHandle>().cloned() else {
                            return;
                        };
                        let screen = cx.new(|cx| {
                            PlaylistScreen::new(PlaylistSource::LikedSongs, client.clone(), cx)
                        });
                        router.push(screen.into(), cx);
                    })
                    .child(
                        div()
                            .size(px(40.0))
                            .rounded(px(4.0))
                            .bg(rgb(0x5c44c7))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("♥"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child("Liked Songs")
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xb3b3b3))
                                    .child("Playlist · Saved tracks"),
                            ),
                    ),
            )
            .child(
                div()
                    .id("library-entries")
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .children(entries),
            )
            .children(self.entries.is_empty().then(|| {
                div()
                    .py(px(8.0))
                    .text_color(rgb(0xb3b3b3))
                    .text_sm()
                    .child("Your saved playlists will appear here.")
            }))
    }
}

/// Bottom playback bar backed by the local Spotify Connect device.
struct PlayerBar {
    client: NativeSpotify,
    playback: PlaybackSnapshot,
}

impl PlayerBar {
    fn new(client: NativeSpotify, cx: &mut Context<Self>) -> Self {
        let playback = client.playback();
        let poll_client = client.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let snapshot = poll_client.playback();
                let updated = this.update(cx, |this, cx| {
                    if this.playback != snapshot {
                        this.playback = snapshot;
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self { client, playback }
    }
}

impl Render for PlayerBar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let playing = self.playback.is_playing;
        let track = self.playback.now_playing.clone();
        let position = self.playback.position_ms;
        let duration = track.as_ref().map(|t| t.duration_ms).unwrap_or(0);
        let progress = if duration > 0 {
            (position as f32 / duration as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let volume = self.playback.volume_percent;
        let client = self.client.clone();

        let prev_client = client.clone();
        let play_client = client.clone();
        let next_client = client.clone();
        let volume_client = client.clone();

        div()
            .id("player-bar")
            .mt(px(8.0))
            .h(px(76.0))
            .w_full()
            .flex()
            .items_center()
            .gap(px(16.0))
            .px(px(16.0))
            .bg(rgb(0x000000))
            .border_t_1()
            .border_color(rgb(0x1f1f1f))
            .child(
                // Track info
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .w(px(300.0))
                    .flex_shrink_0()
                    .child(match track.as_ref().and_then(|t| t.cover_url.clone()) {
                        Some(url) => img(url).size(px(56.0)).rounded(px(6.0)).into_any_element(),
                        None => div()
                            .size(px(56.0))
                            .rounded(px(6.0))
                            .bg(rgb(0x282828))
                            .into_any_element(),
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(
                                        track
                                            .as_ref()
                                            .map(|t| t.title.clone())
                                            .unwrap_or_else(|| "Nothing playing".into()),
                                    ),
                            )
                            .child(
                                div().text_xs().text_color(rgb(0xb3b3b3)).child(
                                    track.as_ref().map(|t| t.artists.clone()).unwrap_or_else(
                                        || "Search for a song to start listening".into(),
                                    ),
                                ),
                            ),
                    ),
            )
            .child(
                // Center controls + progress
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(18.0))
                            .child(icon_control_button(
                                "icons/previous.svg",
                                move |client| {
                                    let _ = client.previous();
                                },
                                prev_client,
                            ))
                            .child(control_button(
                                "↺10",
                                move |client| {
                                    let _ = client.seek(position.saturating_sub(10_000));
                                },
                                client.clone(),
                            ))
                            .child(
                                div()
                                    .id("play-pause")
                                    .size(px(38.0))
                                    .rounded_full()
                                    .bg(rgb(0xffffff))
                                    .hover(|style| style.bg(rgb(0xe6e6e6)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(0x000000))
                                    .text_lg()
                                    .on_click(move |_, _, _| {
                                        let _ = play_client.play_pause();
                                    })
                                    .child(
                                        svg()
                                            .path(if playing {
                                                "icons/pause.svg"
                                            } else {
                                                "icons/play.svg"
                                            })
                                            .size(px(18.0))
                                            .text_color(rgb(0x000000)),
                                    ),
                            )
                            .child(control_button(
                                "↻10",
                                move |client| {
                                    let _ = client.seek(position.saturating_add(10_000));
                                },
                                client.clone(),
                            ))
                            .child(icon_control_button(
                                "icons/next.svg",
                                move |client| {
                                    let _ = client.next();
                                },
                                next_client,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .w_full()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0xb3b3b3))
                                    .w(px(40.0))
                                    .child(format_duration(position)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(4.0))
                                    .rounded_full()
                                    .bg(rgb(0x4d4d4d))
                                    .child(
                                        div()
                                            .h_full()
                                            .rounded_full()
                                            .bg(rgb(0xffffff))
                                            .w(relative(progress)),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0xb3b3b3))
                                    .w(px(40.0))
                                    .child(format_duration(duration)),
                            ),
                    ),
            )
            .child(
                // Volume
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .w(px(220.0))
                    .flex_shrink_0()
                    .justify_end()
                    .child(
                        div()
                            .id("volume-down")
                            .cursor_pointer()
                            .px(px(6.0))
                            .text_color(rgb(0xb3b3b3))
                            .on_click(move |_, _, _| {
                                let _ = volume_client.set_volume_percent(volume.saturating_sub(5));
                            })
                            .child("−"),
                    )
                    .child(
                        div()
                            .w(px(120.0))
                            .h(px(4.0))
                            .rounded_full()
                            .bg(rgb(0x4d4d4d))
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(rgb(0xffffff))
                                    .w(relative(volume as f32 / 100.0)),
                            ),
                    )
                    .child(
                        div()
                            .id("volume-up")
                            .cursor_pointer()
                            .px(px(6.0))
                            .text_color(rgb(0xb3b3b3))
                            .on_click({
                                let client = client.clone();
                                move |_, _, _| {
                                    let _ = client.set_volume_percent(volume.saturating_add(5));
                                }
                            })
                            .child("+"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xb3b3b3))
                            .w(px(32.0))
                            .child(format!("{volume}%")),
                    ),
            )
    }
}

fn control_button(
    label: &'static str,
    action: impl Fn(&NativeSpotify) + 'static,
    client: NativeSpotify,
) -> impl IntoElement {
    div()
        .id(label)
        .cursor_pointer()
        .text_color(rgb(0xd9d9d9))
        .hover(|style| style.text_color(rgb(0xffffff)))
        .on_click(move |_, _, _| action(&client))
        .child(label)
}

fn icon_control_button(
    path: &'static str,
    action: impl Fn(&NativeSpotify) + 'static,
    client: NativeSpotify,
) -> impl IntoElement {
    div()
        .id(path)
        .cursor_pointer()
        .text_color(rgb(0xd9d9d9))
        .hover(|style| style.text_color(rgb(0xffffff)))
        .on_click(move |_, _, _| action(&client))
        .child(svg().path(path).size(px(18.0)).text_color(rgb(0xd9d9d9)))
}

fn format_duration(ms: u64) -> String {
    let total_seconds = ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}
