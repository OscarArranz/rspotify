use std::time::{Duration, Instant};

use futures::channel::oneshot;
use gpui::{
    AnyElement, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Render,
    StatefulInteractiveElement, Styled, Subscription, Window, canvas, div, img,
    prelude::FluentBuilder, px, relative, rgb, svg,
};

use crate::lib::spotify::{LikedStatus, NativeSpotify, PlaybackSnapshot};
use crate::router::{StackRouter, StackRouterHandle};
use crate::state::AppState;
use crate::view::components::library::{LibrarySidebar, LibrarySize};
use crate::view::components::liked::{MUTED, like_plus, liked_check};
use crate::view::components::render_cache::{cached_bar, cached_fill};
use crate::view::components::resizable::{ResizableState, h_resizable, resizable_panel};
use crate::view::screens::home::HomeScreen;

const PANEL_MARGIN: f32 = 8.0;

/// Main layout containing the stack router, library and player bar.
pub struct MainLayout {
    stack_router: Entity<StackRouter>,
    library: Entity<LibrarySidebar>,
    player: Option<Entity<PlayerBar>>,
    client: Option<NativeSpotify>,
    panels: Entity<ResizableState>,
    library_size: LibrarySize,
    normal_library_width: gpui::Pixels,
    _library_subscription: Subscription,
    has_client: bool,
    _auth_subscription: Subscription,
}

impl MainLayout {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Create the stack router with HomeScreen as initial screen
        let home = cx.new(HomeScreen::new);
        let stack_router = cx.new(|_| StackRouter::with_initial(home.into()));

        // Register stack router globally so child components can navigate
        let stack_handle = StackRouterHandle::new(stack_router.clone());
        cx.set_global(stack_handle);

        let client = cx
            .try_global::<AppState>()
            .and_then(AppState::spotify_client);
        let library = cx.new(|cx| LibrarySidebar::new(client.clone(), window, cx));
        let panels = cx.new(|_| ResizableState::default());
        let library_subscription = cx.subscribe(&library, |this, _, size: &LibrarySize, cx| {
            if this.library_size == LibrarySize::Normal {
                this.normal_library_width = this
                    .panels
                    .read(cx)
                    .sizes()
                    .first()
                    .copied()
                    .unwrap_or(px(316.0));
            }
            this.library_size = *size;
            let width = match size {
                LibrarySize::Normal => this.normal_library_width,
                LibrarySize::Collapsed => px(80.0),
                LibrarySize::Expanded => this.normal_library_width,
            };
            if *size != LibrarySize::Expanded {
                this.panels
                    .update(cx, |panels, cx| panels.set_first_size(width, cx));
            }
            cx.notify();
        });
        let player = client
            .clone()
            .map(|client| cx.new(|cx| PlayerBar::new(client, cx)));
        let has_client = player.is_some();
        let auth_subscription = cx.observe_global::<AppState>(|this, cx| {
            this.attach_authenticated_client(cx);
            cx.notify();
        });

        Self {
            stack_router,
            library,
            player,
            client,
            panels,
            library_size: LibrarySize::Normal,
            normal_library_width: px(316.0),
            _library_subscription: library_subscription,
            has_client,
            _auth_subscription: auth_subscription,
        }
    }

    fn attach_authenticated_client(&mut self, cx: &mut Context<Self>) {
        if self.has_client {
            return;
        }
        let Some(client) = cx
            .try_global::<AppState>()
            .and_then(AppState::spotify_client)
        else {
            return;
        };
        self.library
            .update(cx, |library, cx| library.attach_client(client.clone(), cx));
        self.player = Some(cx.new(|cx| PlayerBar::new(client.clone(), cx)));
        self.client = Some(client);
        self.has_client = true;
    }
}

impl Render for MainLayout {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "space"
                    && !event.keystroke.modifiers.modified()
                    && !event.is_held
                    && window.focused(cx).is_none()
                    && let Some(client) = &this.client
                {
                    let _ = client.play_pause();
                    cx.stop_propagation();
                }
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x000000))
            .text_color(rgb(0xffffff))
            .px(px(PANEL_MARGIN))
            .pb(px(PANEL_MARGIN))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .when(self.library_size == LibrarySize::Expanded, |d| {
                        d.child(cached_fill(self.library.clone()))
                    })
                    .when(self.library_size != LibrarySize::Expanded, |d| {
                        d.child(
                            h_resizable("main-layout")
                                .with_state(&self.panels)
                                .child(
                                    resizable_panel()
                                        .size(px(316.0))
                                        .size_range(
                                            if self.library_size == LibrarySize::Collapsed {
                                                px(80.0)..px(80.0)
                                            } else {
                                                px(240.0)..px(800.0)
                                            },
                                        )
                                        .child(
                                            div()
                                                .size_full()
                                                .pr(px(PANEL_MARGIN))
                                                .child(cached_fill(self.library.clone())),
                                        ),
                                )
                                .child(
                                    resizable_panel()
                                        .size_range(px(240.0)..gpui::Pixels::MAX)
                                        .child(cached_fill(self.stack_router.clone())),
                                ),
                        )
                    }),
            )
            .children(self.player.clone().map(|player| cached_bar(player, 76.0)))
    }
}

/// Bottom playback bar backed by the local Spotify Connect device.
struct PlayerBar {
    client: NativeSpotify,
    playback: PlaybackSnapshot,
    seek_slider: Slider,
    volume_slider: Slider,
    dragging: Option<SliderDrag>,
    optimistic_seek: Option<OptimisticSeek>,
    optimistic_volume: Option<OptimisticVolume>,
    last_nonzero_volume: u8,
    liked_revision: u64,
    like_pending_uri: Option<String>,
    like_override: Option<(String, bool)>,
    like_error: Option<LikeError>,
}

struct LikeError {
    uri: String,
    message: String,
    at: Instant,
}

struct OptimisticSeek {
    target_ms: u64,
    started_at: Instant,
    track_uri: String,
}

struct OptimisticVolume {
    target_percent: u8,
    started_at: Instant,
}

#[derive(Default)]
struct Slider {
    bounds: Option<Bounds<Pixels>>,
    hover: Option<f32>,
}

impl Slider {
    fn fraction_at(&self, x: Pixels) -> Option<f32> {
        self.bounds.map(|bounds| slider_fraction(bounds, x))
    }

    fn contains(&self, point: &gpui::Point<Pixels>) -> bool {
        self.bounds.is_some_and(|bounds| bounds.contains(point))
    }

    fn update_hover(&mut self, x: Pixels) {
        self.hover = self.fraction_at(x);
    }
}

#[derive(Clone, Copy)]
enum SliderDrag {
    Seek,
    Volume,
}

impl PlayerBar {
    fn new(client: NativeSpotify, cx: &mut Context<Self>) -> Self {
        let playback = client.playback();
        let last_nonzero_volume = if playback.volume_percent > 0 {
            playback.volume_percent
        } else {
            50
        };
        let liked_revision = client.liked_revision();
        client.ensure_liked_tracks_loaded();
        let poll_client = client.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                poll_client.ensure_liked_tracks_loaded();
                let snapshot = poll_client.playback();
                let liked_revision = poll_client.liked_revision();
                let updated = this.update(cx, |this, cx| {
                    let track_changed = this
                        .playback
                        .now_playing
                        .as_ref()
                        .map(|track| track.uri.as_str())
                        != snapshot
                            .now_playing
                            .as_ref()
                            .map(|track| track.uri.as_str());
                    if track_changed {
                        this.like_pending_uri = None;
                        this.like_override = None;
                        this.like_error = None;
                    }
                    let error_expired = this
                        .like_error
                        .as_ref()
                        .is_some_and(|error| error.at.elapsed() >= Duration::from_secs(5));
                    if error_expired {
                        this.like_error = None;
                    }
                    if this.playback != snapshot
                        || this.liked_revision != liked_revision
                        || error_expired
                    {
                        if snapshot.volume_percent > 0 {
                            this.last_nonzero_volume = snapshot.volume_percent;
                        }
                        this.playback = snapshot;
                        this.liked_revision = liked_revision;
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            client,
            playback,
            seek_slider: Slider::default(),
            volume_slider: Slider::default(),
            dragging: None,
            optimistic_seek: None,
            optimistic_volume: None,
            last_nonzero_volume,
            liked_revision,
            like_pending_uri: None,
            like_override: None,
            like_error: None,
        }
    }

    fn render_like_control(
        &self,
        uri: String,
        liked: Option<bool>,
        pending: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let unknown = liked.is_none();
        let icon = match liked {
            Some(true) => liked_check(22.0),
            _ => like_plus(22.0, if unknown { 0x5a5a5a } else { MUTED }),
        };
        div()
            .id("player-like")
            .flex_shrink_0()
            .size(px(26.0))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .opacity(if pending || unknown { 0.45 } else { 1.0 })
            .hover(|style| style.opacity(1.0))
            .on_click(cx.listener(move |this, _, _, cx| {
                if pending || this.like_pending_uri.is_some() {
                    return;
                }
                let Some(currently_liked) = liked else {
                    this.client.ensure_liked_tracks_loaded();
                    cx.notify();
                    return;
                };
                this.toggle_liked(uri.clone(), currently_liked, cx);
            }))
            .child(icon)
            .into_any_element()
    }

    fn toggle_liked(&mut self, uri: String, currently_liked: bool, cx: &mut Context<Self>) {
        if self.like_pending_uri.is_some() {
            return;
        }
        let target = !currently_liked;
        let client = self.client.clone();
        let worker_uri = uri.clone();
        self.like_pending_uri = Some(uri.clone());
        self.like_override = Some((uri.clone(), target));
        self.like_error = None;
        let (tx, rx) = oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(client.set_liked(&worker_uri, target));
        });
        cx.spawn(async move |this, cx| {
            let result = rx.await;
            let _ = this.update(cx, |this, cx| {
                if this.like_pending_uri.as_deref() == Some(uri.as_str()) {
                    this.like_pending_uri = None;
                }
                if this
                    .like_override
                    .as_ref()
                    .is_some_and(|(pending, _)| pending == &uri)
                {
                    this.like_override = None;
                }
                match result {
                    Ok(Ok(())) => {
                        this.liked_revision = this.client.liked_revision();
                    }
                    Ok(Err(_)) => {
                        this.like_error = Some(LikeError {
                            uri: uri.clone(),
                            message: "Couldn't update Liked Songs".into(),
                            at: Instant::now(),
                        });
                        eprintln!("liked songs update failed");
                    }
                    Err(_) => {
                        this.like_error = Some(LikeError {
                            uri: uri.clone(),
                            message: "Couldn't update Liked Songs".into(),
                            at: Instant::now(),
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for PlayerBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let server_track_uri = self
            .playback
            .now_playing
            .as_ref()
            .map(|track| track.uri.as_str());
        if self.optimistic_seek.as_ref().is_some_and(|pending| {
            now.duration_since(pending.started_at) >= Duration::from_secs(3)
                || Some(pending.track_uri.as_str()) != server_track_uri
                || self.playback.position_ms.abs_diff(pending.target_ms) <= 1_000
        }) {
            self.optimistic_seek = None;
        }
        if self.optimistic_volume.as_ref().is_some_and(|pending| {
            now.duration_since(pending.started_at) >= Duration::from_secs(3)
                || self.playback.volume_percent == pending.target_percent
        }) {
            self.optimistic_volume = None;
        }

        let playing = self.playback.is_playing;
        let track = self.playback.now_playing.clone();
        let like_uri = track
            .as_ref()
            .map(|track| track.uri.clone())
            .unwrap_or_default();
        let like_supported = like_uri.starts_with("spotify:track:");
        let like_base = if like_supported {
            self.client.liked_status(&like_uri)
        } else {
            LikedStatus::Unknown
        };
        let like_override = self
            .like_override
            .as_ref()
            .filter(|(uri, _)| uri == &like_uri)
            .map(|(_, liked)| *liked);
        let like_state = like_override.or(match like_base {
            LikedStatus::Liked => Some(true),
            LikedStatus::NotLiked => Some(false),
            LikedStatus::Unknown => None,
        });
        let like_pending = self.like_pending_uri.as_deref() == Some(like_uri.as_str());
        let like_error = self
            .like_error
            .as_ref()
            .filter(|error| error.uri == like_uri && error.at.elapsed() < Duration::from_secs(5))
            .map(|error| error.message.clone());
        let position = self
            .optimistic_seek
            .as_ref()
            .map(|pending| {
                pending.target_ms.saturating_add(if playing {
                    now.duration_since(pending.started_at).as_millis() as u64
                } else {
                    0
                })
            })
            .unwrap_or(self.playback.position_ms);
        let duration = track.as_ref().map(|t| t.duration_ms).unwrap_or(0);
        let progress = if duration > 0 {
            (position as f32 / duration as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let volume = self
            .optimistic_volume
            .as_ref()
            .map(|pending| pending.target_percent)
            .unwrap_or(self.playback.volume_percent);
        let displayed_volume = if matches!(self.dragging, Some(SliderDrag::Volume)) {
            self.volume_slider
                .hover
                .map(|value| (value * 100.0).round() as u8)
                .unwrap_or(volume)
        } else {
            volume
        };
        let client = self.client.clone();

        let prev_client = client.clone();
        let play_client = client.clone();
        let next_client = client.clone();
        let volume_button_client = client.clone();
        let weak = cx.entity().downgrade();
        let down_weak = weak.clone();
        let move_weak = weak.clone();
        let up_weak = weak.clone();
        let seek_layout_weak = weak.clone();
        let volume_layout_weak = weak.clone();
        let seek_move_weak = weak.clone();
        let volume_move_weak = weak;

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
            .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
                down_weak
                    .update(cx, |this, cx| {
                        if this.seek_slider.contains(&event.position) && duration > 0 {
                            if let Some(fraction) = this.seek_slider.fraction_at(event.position.x) {
                                this.dragging = Some(SliderDrag::Seek);
                                this.seek_slider.hover = Some(fraction);
                            }
                        }
                        if this.volume_slider.contains(&event.position) {
                            this.dragging = Some(SliderDrag::Volume);
                            this.volume_slider.update_hover(event.position.x);
                            if let Some(fraction) = this.volume_slider.fraction_at(event.position.x)
                            {
                                this.client
                                    .set_local_volume_percent((fraction * 100.0).round() as u8);
                            }
                        }
                        cx.notify();
                    })
                    .ok();
            })
            .on_mouse_move(move |event: &MouseMoveEvent, _, cx| {
                move_weak
                    .update(cx, |this, cx| {
                        let seek_drag = matches!(this.dragging, Some(SliderDrag::Seek));
                        let volume_drag = matches!(this.dragging, Some(SliderDrag::Volume));
                        if seek_drag || this.seek_slider.contains(&event.position) {
                            this.seek_slider.update_hover(event.position.x);
                        } else {
                            this.seek_slider.hover = None;
                        }
                        if volume_drag || this.volume_slider.contains(&event.position) {
                            this.volume_slider.update_hover(event.position.x);
                        } else {
                            this.volume_slider.hover = None;
                        }
                        if volume_drag {
                            if let Some(fraction) = this.volume_slider.fraction_at(event.position.x)
                            {
                                this.client
                                    .set_local_volume_percent((fraction * 100.0).round() as u8);
                            }
                        }
                        cx.notify();
                    })
                    .ok();
            })
            .on_mouse_up(MouseButton::Left, move |event: &MouseUpEvent, _, cx| {
                up_weak
                    .update(cx, |this, cx| {
                        if matches!(this.dragging, Some(SliderDrag::Seek)) && duration > 0 {
                            if let Some(bounds) = this.seek_slider.bounds {
                                let target_ms = (duration as f64
                                    * slider_fraction(bounds, event.position.x) as f64)
                                    as u64;
                                let _ = this.client.seek(target_ms.min(duration));
                                if let Some(track) = this.playback.now_playing.as_ref() {
                                    this.optimistic_seek = Some(OptimisticSeek {
                                        target_ms: target_ms.min(duration),
                                        started_at: Instant::now(),
                                        track_uri: track.uri.clone(),
                                    });
                                }
                                this.seek_slider.update_hover(event.position.x);
                            }
                        } else if matches!(this.dragging, Some(SliderDrag::Volume)) {
                            if let Some(bounds) = this.volume_slider.bounds {
                                let percent = (slider_fraction(bounds, event.position.x) * 100.0)
                                    .round() as u8;
                                let _ = this.client.set_volume_percent(percent);
                                if percent > 0 {
                                    this.last_nonzero_volume = percent;
                                }
                                this.optimistic_volume = Some(OptimisticVolume {
                                    target_percent: percent,
                                    started_at: Instant::now(),
                                });
                                this.volume_slider.update_hover(event.position.x);
                            }
                        }
                        this.dragging = None;
                        cx.notify();
                    })
                    .ok();
            })
            .child(
                // Track info
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .w(px(320.0))
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
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .child(
                                div()
                                    .truncate()
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
                                div()
                                    .truncate()
                                    .text_xs()
                                    .text_color(if like_error.is_some() {
                                        rgb(0xff6b6b)
                                    } else {
                                        rgb(0xb3b3b3)
                                    })
                                    .child(like_error.unwrap_or_else(|| {
                                        track.as_ref().map(|t| t.artists.clone()).unwrap_or_else(
                                            || "Search for a song to start listening".into(),
                                        )
                                    })),
                            ),
                    )
                    .children(like_supported.then(|| {
                        self.render_like_control(like_uri.clone(), like_state, like_pending, cx)
                    })),
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
                                player_slider(
                                    "seek-slider",
                                    progress,
                                    self.seek_slider.hover,
                                    matches!(self.dragging, Some(SliderDrag::Seek)),
                                    self.seek_slider.hover.map(|fraction| {
                                        format_duration((duration as f64 * fraction as f64) as u64)
                                    }),
                                    move |event, cx| {
                                        seek_move_weak
                                            .update(cx, |this, cx| {
                                                if matches!(this.dragging, Some(SliderDrag::Seek))
                                                    || this.seek_slider.contains(&event.position)
                                                {
                                                    this.seek_slider.update_hover(event.position.x);
                                                    cx.notify();
                                                }
                                            })
                                            .ok();
                                    },
                                    move |bounds, cx| {
                                        seek_layout_weak
                                            .update(cx, |this, _| {
                                                this.seek_slider.bounds = Some(bounds)
                                            })
                                            .ok();
                                    },
                                )
                                .flex_1(),
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
                    .child({
                        let muted = displayed_volume == 0;
                        let icon_path = if muted {
                            "icons/volume-muted.svg"
                        } else {
                            "icons/volume-high.svg"
                        };
                        let restore_volume = self.last_nonzero_volume;
                        div()
                            .id("volume-mute-toggle")
                            .cursor_pointer()
                            .p(px(4.0))
                            .on_click(move |_, _, _| {
                                let target = if muted { restore_volume } else { 0 };
                                let _ = volume_button_client.set_volume_percent(target);
                            })
                            .child(
                                svg()
                                    .path(icon_path)
                                    .size(px(20.0))
                                    .text_color(rgb(0xb3b3b3))
                                    .hover(|style| style.text_color(gpui::white())),
                            )
                    })
                    .child(
                        player_slider(
                            "volume-slider",
                            displayed_volume as f32 / 100.0,
                            self.volume_slider.hover,
                            matches!(self.dragging, Some(SliderDrag::Volume)),
                            None,
                            move |event, cx| {
                                volume_move_weak
                                    .update(cx, |this, cx| {
                                        if matches!(this.dragging, Some(SliderDrag::Volume))
                                            || this.volume_slider.contains(&event.position)
                                        {
                                            this.volume_slider.update_hover(event.position.x);
                                            if matches!(this.dragging, Some(SliderDrag::Volume)) {
                                                if let Some(fraction) =
                                                    this.volume_slider.fraction_at(event.position.x)
                                                {
                                                    this.client.set_local_volume_percent(
                                                        (fraction * 100.0).round() as u8,
                                                    );
                                                }
                                            }
                                            cx.notify();
                                        }
                                    })
                                    .ok();
                            },
                            move |bounds, cx| {
                                volume_layout_weak
                                    .update(cx, |this, _| this.volume_slider.bounds = Some(bounds))
                                    .ok();
                            },
                        )
                        .w(px(120.0)),
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

fn slider_fraction(bounds: Bounds<Pixels>, x: Pixels) -> f32 {
    let width: f32 = bounds.size.width.into();
    let offset: f32 = (x - bounds.origin.x).into();
    if width <= 0.0 {
        0.0
    } else {
        (offset / width).clamp(0.0, 1.0)
    }
}

fn player_slider(
    id: &'static str,
    value: f32,
    hovered_value: Option<f32>,
    dragging: bool,
    tooltip: Option<String>,
    on_pointer_move: impl Fn(&MouseMoveEvent, &mut gpui::App) + 'static,
    on_bounds: impl FnOnce(Bounds<Pixels>, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let preview = if dragging {
        hovered_value.unwrap_or(value)
    } else {
        value
    }
    .clamp(0.0, 1.0);
    let fill_value = if dragging {
        preview
    } else {
        value.clamp(0.0, 1.0)
    };
    let preview_range = hovered_value
        .filter(|hover| !dragging && *hover > value)
        .map(|hover| (value, hover - value));
    let fill_color = if hovered_value.is_some() || dragging {
        0x1ed760
    } else {
        0xffffff
    };

    div()
        .id(id)
        .h(px(12.0))
        .rounded_full()
        .cursor_pointer()
        .flex()
        .items_center()
        .on_mouse_move(move |event: &MouseMoveEvent, _, cx| on_pointer_move(event, cx))
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .h(px(4.0))
                .rounded_full()
                .bg(rgb(0x4d4d4d)),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .h(px(4.0))
                .rounded_full()
                .bg(rgb(fill_color))
                .w(relative(fill_value)),
        )
        .children(preview_range.map(|(start, width)| {
            div()
                .absolute()
                .left(relative(start))
                .h(px(4.0))
                .rounded_full()
                .bg(rgb(0xb3b3b3))
                .w(relative(width))
        }))
        .children((hovered_value.is_some() || dragging).then(|| {
            div()
                .absolute()
                .left(relative(preview))
                .ml(px(-5.0))
                .size(px(10.0))
                .rounded_full()
                .bg(rgb(0xffffff))
        }))
        .children(tooltip.map(|text| {
            div()
                .absolute()
                .left(relative(preview))
                .bottom(px(14.0))
                .ml(px(-23.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(5.0))
                .bg(rgb(0x282828))
                .text_color(rgb(0xffffff))
                .text_sm()
                .child(text)
        }))
        .child(
            canvas(move |bounds, _, cx| on_bounds(bounds, cx), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
}
