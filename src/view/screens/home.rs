//! Feed-backed native Home. UI state and scroll positions live in this entity.

use std::{collections::HashMap, sync::Arc};

use futures::channel::oneshot;
use gpui::{
    AnyElement, AppContext, Context, FontWeight, InteractiveElement, IntoElement, ObjectFit,
    ParentElement, Render, RenderImage, ScrollHandle, SharedString, StatefulInteractiveElement,
    Styled, StyledImage, Window, canvas, div, img, point, prelude::FluentBuilder, px, rgb, svg,
};

use crate::{
    lib::spotify::{
        NativeSpotify, PlaylistSource,
        home::{HomeFeed, HomeItem, HomePanel, HomeShelf},
    },
    router::StackRouterHandle,
    state::AppState,
    view::{
        components::smooth_scroll::{ScrollAxis, SmoothScrollElement, SmoothScrollState},
        screens::playlist::{PlaylistScreen, render_cover},
    },
};

pub(crate) struct HomeScreen {
    client: Option<NativeSpotify>,
    feed: HomeFeed,
    facet: String,
    loading: bool,
    failed: bool,
    message: Option<String>,
    generation: u64,
    width: f32,
    scroll: ScrollHandle,
    scroll_state: SmoothScrollState,
    shelf_scrolls: Vec<ScrollHandle>,
    shelf_states: Vec<SmoothScrollState>,
    expanded: Option<usize>,
    shelf_loading: bool,
    hovered_panel: Option<usize>,
    previews: HashMap<usize, Option<Arc<RenderImage>>>,
    preview_pending: std::collections::HashSet<usize>,
    _auth: gpui::Subscription,
}

impl HomeScreen {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let auth = cx.observe_global::<AppState>(|this, cx| this.attach(cx));
        let mut screen = Self {
            client: None,
            feed: HomeFeed::default(),
            facet: String::new(),
            loading: false,
            failed: false,
            message: None,
            generation: 0,
            width: 800.0,
            scroll: ScrollHandle::new(),
            scroll_state: SmoothScrollState::new(cx.entity_id()),
            shelf_scrolls: Vec::new(),
            shelf_states: Vec::new(),
            expanded: None,
            shelf_loading: false,
            hovered_panel: None,
            previews: HashMap::new(),
            preview_pending: Default::default(),
            _auth: auth,
        };
        screen.attach(cx);
        screen
    }

    fn attach(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            self.client = cx
                .try_global::<AppState>()
                .and_then(AppState::spotify_client);
            if self.client.is_some() {
                self.load(cx);
            }
        }
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        let facet = self.facet.clone();
        self.loading = true;
        self.failed = false;
        self.message = None;
        self.hovered_panel = None;
        self.previews.clear();
        self.preview_pending.clear();
        self.shelf_loading = false;
        cx.spawn(async move |this, cx| {
            let (tx, rx) = oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(client.home(&facet));
            });
            let result = rx.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(Ok(feed)) => {
                        this.shelf_scrolls =
                            feed.shelves.iter().map(|_| ScrollHandle::new()).collect();
                        this.shelf_states = feed
                            .shelves
                            .iter()
                            .map(|_| SmoothScrollState::new(cx.entity_id()))
                            .collect();
                        this.feed = feed;
                        this.expanded = None;
                        this.scroll.set_offset(point(px(0.0), px(0.0)));
                    }
                    _ => this.failed = true,
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn play(&mut self, item: HomeItem, cx: &mut Context<Self>) {
        if !item.playable {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.hovered_panel = None;
        cx.spawn(async move |this, cx| {
            let (tx, rx) = oneshot::channel();
            std::thread::spawn(move || {
                let result = if item.kind == "LikedSongs" {
                    client.play_context(&client.liked_songs_context_uri())
                } else if matches!(item.kind.as_str(), "Track" | "Episode") {
                    client.play_track(&item.uri)
                } else {
                    client.play_context(&item.uri)
                };
                let _ = tx.send(result.is_ok());
            });
            if !matches!(rx.await, Ok(true)) {
                let _ = this.update(cx, |this, cx| {
                    this.message =
                        Some("Couldn't start playback. Spotify Premium is required.".into());
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn open(&mut self, item: HomeItem, cx: &mut Context<Self>) {
        self.hovered_panel = None;
        if matches!(
            item.kind.as_str(),
            "Playlist" | "LikedSongs" | "Album" | "Artist" | "Podcast"
        ) {
            let Some(client) = self.client.clone() else {
                return;
            };
            let Some(router) = cx.try_global::<StackRouterHandle>().cloned() else {
                return;
            };
            let source = if item.kind == "LikedSongs" {
                PlaylistSource::LikedSongs
            } else if item.kind == "Playlist" {
                PlaylistSource::Playlist {
                    uri: item.uri.clone(),
                    name: item.title,
                    owner: String::new(),
                    image_url: item.image,
                }
            } else {
                let kind = match item.kind.as_str() {
                    "Album" => "Album",
                    "Artist" => "Artist",
                    _ => "Podcast",
                };
                PlaylistSource::Context {
                    uri: item.uri.clone(),
                    name: item.title,
                    owner: item.subtitle,
                    image_url: item.image,
                    kind,
                }
            };
            let screen = cx.new(|cx| PlaylistScreen::new(source, client, cx));
            router.push_for_source(screen.into(), item.uri, cx);
        } else {
            self.play(item, cx);
        }
    }

    fn preview(&mut self, index: usize, hovered: bool, cx: &mut Context<Self>) {
        self.hovered_panel = hovered.then_some(index);
        if !hovered || self.previews.contains_key(&index) {
            cx.notify();
            return;
        }
        let Some(url) = self
            .feed
            .panels
            .get(index)
            .and_then(|p| p.video_url.clone())
        else {
            return;
        };
        self.previews.insert(index, None);
        self.preview_pending.insert(index);
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let (tx, rx) = oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(super::home_video::decode(&url));
            });
            let preview = rx.await.ok().flatten();
            let _ = this.update(cx, |this, cx| {
                if generation == this.generation {
                    this.preview_pending.remove(&index);
                    this.previews.insert(index, preview);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn expand_shelf(&mut self, index: usize, more: bool, cx: &mut Context<Self>) {
        if self.shelf_loading {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(shelf) = self.feed.shelves.get(index) else {
            return;
        };
        let offset = if more {
            let Some(offset) = shelf.next_offset else {
                return;
            };
            offset
        } else {
            0
        };
        let uri = shelf.uri.clone();
        let generation = self.generation;
        self.expanded = Some(index);
        self.hovered_panel = None;
        self.shelf_loading = true;
        if !more {
            self.scroll.set_offset(point(px(0.0), px(0.0)));
        }
        cx.spawn(async move |this, cx| {
            let (tx, rx) = oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(client.home_section(&uri, offset));
            });
            let result = rx.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.shelf_loading = false;
                match result {
                    Ok(Ok((items, next))) => {
                        let shelf = &mut this.feed.shelves[index];
                        if !more {
                            shelf.items = items;
                        } else {
                            for item in items {
                                if !shelf.items.iter().any(|i| i.uri == item.uri) {
                                    shelf.items.push(item);
                                }
                            }
                        }
                        shelf.next_offset = next;
                    }
                    _ => {
                        this.message =
                            Some("Couldn't load the full shelf. Try Show all again.".into())
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn play_button(
        &self,
        item: &HomeItem,
        id: String,
        size: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        let item = item.clone();
        div()
            .id(SharedString::from(id))
            .size(px(size))
            .rounded_full()
            .bg(rgb(0x1ed760))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x3be477)))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.play(item.clone(), cx);
            }))
            .child(
                svg()
                    .path("icons/play.svg")
                    .size(px(size * 0.45))
                    .text_color(rgb(0x000000)),
            )
            .into_any_element()
    }

    fn share_button(&self, item: &HomeItem, id: String, cx: &Context<Self>) -> AnyElement {
        let uri = item.uri.clone();
        div()
            .id(SharedString::from(id))
            .size(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(rgb(0xb3b3b3))
            .hover(|s| s.text_color(rgb(0xffffff)))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                let parts: Vec<_> = uri.split(':').collect();
                let link = if parts.len() == 3 {
                    format!("https://open.spotify.com/{}/{}", parts[1], parts[2])
                } else {
                    uri.clone()
                };
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(link));
                this.message = Some("Link copied".into());
                cx.notify();
            }))
            .child(svg().path("icons/share.svg").size(px(24.0)))
            .into_any_element()
    }

    fn card(&self, item: &HomeItem, id: String, size: f32, cx: &Context<Self>) -> AnyElement {
        let open = item.clone();
        div()
            .id(SharedString::from(id.clone()))
            .group("home-card")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(size + 24.0))
            .p(px(12.0))
            .gap(px(8.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x242424)))
            .on_click(cx.listener(move |this, _, _, cx| this.open(open.clone(), cx)))
            .child(
                div()
                    .relative()
                    .size(px(size))
                    .rounded(px(if item.kind == "Artist" {
                        size / 2.0
                    } else {
                        6.0
                    }))
                    .overflow_hidden()
                    .child(render_cover(
                        item.image.as_deref(),
                        size,
                        item.kind == "LikedSongs",
                        0x262626,
                    ))
                    .when(item.playable, |s| {
                        s.child(
                            div()
                                .absolute()
                                .bottom(px(8.0))
                                .right(px(8.0))
                                .opacity(0.0)
                                .group_hover("home-card", |s| s.opacity(1.0))
                                .child(self.play_button(item, format!("{id}-play"), 48.0, cx)),
                        )
                    }),
            )
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_ellipsis()
                    .child(item.title.clone()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xb3b3b3))
                    .h(px(40.0))
                    .overflow_hidden()
                    .child(item.subtitle.clone()),
            )
            .into_any_element()
    }

    fn shelf(&self, index: usize, shelf: &HomeShelf, cx: &Context<Self>) -> AnyElement {
        let size = if self.width < 650.0 { 150.0 } else { 180.0 };
        let spotlight = matches!(
            shelf.layout.as_str(),
            "HomeSpotlightSectionData" | "HomePromotionSectionData"
        );
        let handle = self.shelf_scrolls[index].clone();
        let left = handle.clone();
        let right = handle.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .w_full()
            .when(!shelf.subtitle.is_empty(), |s| {
                s.child(
                    div()
                        .px(px(12.0))
                        .text_sm()
                        .text_color(rgb(0xb3b3b3))
                        .child(shelf.subtitle.clone()),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(12.0))
                    .gap(px(16.0))
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .child(shelf.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(12.0))
                            .items_center()
                            .child(
                                div()
                                    .id(("shelf-prev", index))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        let offset = left.offset();
                                        left.set_offset(point(
                                            (offset.x + px(420.0)).min(px(0.0)),
                                            offset.y,
                                        ));
                                        cx.refresh_windows();
                                    })
                                    .child(svg().path("icons/back.svg").size(px(20.0))),
                            )
                            .child(
                                div()
                                    .id(("shelf-next", index))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        let offset = right.offset();
                                        right.set_offset(point(
                                            (offset.x - px(420.0)).max(-right.max_offset().width),
                                            offset.y,
                                        ));
                                        cx.refresh_windows();
                                    })
                                    .child(svg().path("icons/forward.svg").size(px(20.0))),
                            )
                            .child(
                                div()
                                    .id(("shelf-all", index))
                                    .cursor_pointer()
                                    .text_sm()
                                    .text_color(rgb(0xb3b3b3))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.expand_shelf(index, false, cx)
                                    }))
                                    .child("Show all"),
                            ),
                    ),
            )
            .child(if spotlight {
                self.featured(&shelf.items[0], index, cx)
            } else {
                div()
                    .id(("home-shelf", index))
                    .flex()
                    .w_full()
                    .overflow_x_scroll()
                    .smooth_scroll(
                        handle,
                        ScrollAxis::Horizontal,
                        self.shelf_states[index].clone(),
                    )
                    .children(
                        shelf.items.iter().enumerate().map(|(i, item)| {
                            self.card(item, format!("shelf-{index}-{i}"), size, cx)
                        }),
                    )
                    .into_any_element()
            })
            .into_any_element()
    }

    fn featured(&self, item: &HomeItem, index: usize, cx: &Context<Self>) -> AnyElement {
        let open = item.clone();
        let size = if self.width < 650.0 { 140.0 } else { 220.0 };
        div()
            .id(("featured", index))
            .flex()
            .items_center()
            .gap(px(24.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x242424)))
            .on_click(cx.listener(move |this, _, _, cx| this.open(open.clone(), cx)))
            .child(render_cover(item.image.as_deref(), size, false, 0x262626))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xb3b3b3))
                            .child(item.kind.clone()),
                    )
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::BOLD)
                            .child(item.title.clone()),
                    )
                    .child(div().text_color(rgb(0xb3b3b3)).child(item.subtitle.clone()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(16.0))
                            .when(item.playable, |s| {
                                s.child(self.play_button(
                                    item,
                                    format!("featured-play-{index}"),
                                    48.0,
                                    cx,
                                ))
                            })
                            .child(self.share_button(item, format!("featured-share-{index}"), cx)),
                    ),
            )
            .into_any_element()
    }

    fn panel(&self, index: usize, panel: &HomePanel, cx: &Context<Self>) -> AnyElement {
        let width = (self.width - 100.0).clamp(220.0, 620.0);
        let height = (width * 1.05).min(650.0);
        let open = panel.item.clone();
        let preview = self
            .previews
            .get(&index)
            .and_then(|p| p.clone())
            .filter(|_| self.hovered_panel == Some(index));
        let poster = panel.poster.as_ref().or(panel.item.image.as_ref());
        let media = if let Some(image) = preview {
            img(image)
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        } else if let Some(url) = poster {
            img(url.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        } else {
            div().size_full().bg(rgb(0x263d36)).into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .items_center()
            .w_full()
            .child(
                div()
                    .w(px(width))
                    .text_sm()
                    .text_color(rgb(0xb3b3b3))
                    .child(panel.label.clone()),
            )
            .child(
                div()
                    .id(("home-panel", index))
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .rounded(px(12.0))
                    .overflow_hidden()
                    .bg(rgb(0x202020))
                    .cursor_pointer()
                    .on_hover(
                        cx.listener(move |this, hovered, _, cx| this.preview(index, *hovered, cx)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.open(open.clone(), cx)))
                    .child(div().absolute().size_full().child(media))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .w_full()
                            .p(px(24.0))
                            .bg(gpui::rgba(0x00000099))
                            .flex()
                            .gap(px(16.0))
                            .items_center()
                            .child(render_cover(
                                panel.item.image.as_deref(),
                                80.0,
                                false,
                                0x262626,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_2xl()
                                            .font_weight(FontWeight::BOLD)
                                            .child(panel.item.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0xb3b3b3))
                                            .child(panel.item.kind.clone()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .w_full()
                            .p(px(24.0))
                            .bg(gpui::rgba(0x000000bb))
                            .flex()
                            .flex_col()
                            .gap(px(20.0))
                            .child(div().child(panel.item.subtitle.clone()))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div().text_sm().child(
                                            if self.preview_pending.contains(&index) {
                                                "Preparing muted preview…"
                                            } else if self
                                                .previews
                                                .get(&index)
                                                .is_some_and(Option::is_none)
                                            {
                                                "Preview unavailable · artwork shown"
                                            } else if panel.video_url.is_some() {
                                                "Muted Canvas · hover to preview"
                                            } else {
                                                "Recommended for you"
                                            },
                                        ),
                                    )
                                    .when(panel.item.playable, |s| {
                                        s.child(self.play_button(
                                            &panel.item,
                                            format!("panel-play-{index}"),
                                            56.0,
                                            cx,
                                        ))
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for HomeScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        let measure = canvas(
            move |bounds, _, cx| {
                let _ = entity.update(cx, |this, cx| {
                    let width: f32 = bounds.size.width.into();
                    if (this.width - width).abs() > 1.0 {
                        this.width = width;
                        cx.notify();
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let mut chips = vec![(String::new(), "All".to_string())];
        chips.extend(
            self.feed
                .chips
                .iter()
                .map(|c| (c.id.clone(), c.label.clone())),
        );
        let content =
            if self.loading {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(28.0))
                    .child(div().text_color(rgb(0xb3b3b3)).child("Loading your Home…"))
                    .children((0..3).map(|_| {
                        div()
                            .h(px(200.0))
                            .w_full()
                            .rounded(px(8.0))
                            .bg(rgb(0x242424))
                    }))
                    .into_any_element()
            } else if self.failed {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .p(px(12.0))
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Couldn't load Home"),
                    )
                    .child("Your session or Spotify's Home service may be temporarily unavailable.")
                    .child(
                        div()
                            .id("home-retry")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| this.load(cx)))
                            .child("Try again"),
                    )
                    .into_any_element()
            } else if let Some(index) = self.expanded {
                let shelf = &self.feed.shelves[index];
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(
                        div()
                            .id("home-shelf-back")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.expanded = None;
                                cx.notify();
                            }))
                            .child("Back to Home"),
                    )
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::BOLD)
                            .child(shelf.title.clone()),
                    )
                    .child(
                        div().flex().flex_wrap().children(
                            shelf.items.iter().enumerate().map(|(i, item)| {
                                self.card(item, format!("expanded-{i}"), 180.0, cx)
                            }),
                        ),
                    )
                    .when(self.shelf_loading, |s| s.child("Loading recommendations…"))
                    .when(shelf.next_offset.is_some() && !self.shelf_loading, |s| {
                        s.child(
                            div()
                                .id("home-load-more")
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.expand_shelf(index, true, cx)
                                }))
                                .child("Load more"),
                        )
                    })
                    .into_any_element()
            } else {
                let columns = if self.width >= 1050.0 { 4 } else { 2 };
                let shortcut_width =
                    ((self.width - 64.0 - (columns - 1) as f32 * 8.0) / columns as f32).max(100.0);
                div()
                    .flex()
                    .flex_col()
                    .gap(px(32.0))
                    .child(
                        div().flex().flex_wrap().gap(px(8.0)).px(px(12.0)).children(
                            self.feed
                                .shortcuts
                                .iter()
                                .take(8)
                                .enumerate()
                                .map(|(i, item)| {
                                    let open = item.clone();
                                    let id = format!("shortcut-{i}");
                                    div()
                                        .id(SharedString::from(id.clone()))
                                        .group("shortcut")
                                        .relative()
                                        .flex()
                                        .items_center()
                                        .gap(px(12.0))
                                        .w(px(shortcut_width))
                                        .h(px(64.0))
                                        .rounded(px(5.0))
                                        .overflow_hidden()
                                        .bg(rgb(0x2d2d2d))
                                        .hover(|s| s.bg(rgb(0x454545)))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.open(open.clone(), cx)
                                        }))
                                        .child(render_cover(
                                            item.image.as_deref(),
                                            64.0,
                                            item.kind == "LikedSongs",
                                            0x313131,
                                        ))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w(px(0.0))
                                                .pr(px(8.0))
                                                .font_weight(FontWeight::BOLD)
                                                .text_ellipsis()
                                                .child(item.title.clone()),
                                        )
                                        .when(item.playable, |s| {
                                            s.child(
                                                div()
                                                    .absolute()
                                                    .right(px(8.0))
                                                    .opacity(0.0)
                                                    .group_hover("shortcut", |s| s.opacity(1.0))
                                                    .child(self.play_button(
                                                        item,
                                                        format!("{id}-play"),
                                                        40.0,
                                                        cx,
                                                    )),
                                            )
                                        })
                                }),
                        ),
                    )
                    .children(
                        self.feed
                            .shelves
                            .iter()
                            .enumerate()
                            .map(|(i, shelf)| self.shelf(i, shelf, cx)),
                    )
                    .children(
                        self.feed
                            .panels
                            .iter()
                            .enumerate()
                            .map(|(i, panel)| self.panel(i, panel, cx)),
                    )
                    .when(
                        self.feed.shelves.is_empty() && self.feed.shortcuts.is_empty(),
                        |s| s.child("No recommendations available for this category yet."),
                    )
                    .child(
                        div()
                            .px(px(12.0))
                            .py(px(24.0))
                            .text_sm()
                            .text_color(rgb(0xb3b3b3))
                            .child("Playback requires Spotify Premium."),
                    )
                    .into_any_element()
            };
        div()
            .relative()
            .size_full()
            .min_w(px(0.0))
            .bg(rgb(0x181818))
            .rounded(px(8.0))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(measure)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(32.0))
                    .py(px(16.0))
                    .bg(rgb(0x291c19))
                    .children(chips.into_iter().map(|(id, label)| {
                        let active = self.facet == id;
                        div()
                            .id(SharedString::from(format!("home-chip-{id}")))
                            .rounded_full()
                            .px(px(16.0))
                            .py(px(9.0))
                            .bg(rgb(if active { 0xffffff } else { 0x38302e }))
                            .text_color(rgb(if active { 0x000000 } else { 0xffffff }))
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.85))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.facet != id {
                                    this.facet = id.clone();
                                    this.load(cx);
                                }
                            }))
                            .child(label)
                    })),
            )
            .children(self.message.clone().map(|message| {
                div()
                    .px(px(32.0))
                    .py(px(8.0))
                    .text_color(rgb(0xffce81))
                    .child(message)
            }))
            .child(
                div()
                    .id("home-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .overflow_y_scroll()
                    .smooth_scroll(
                        self.scroll.clone(),
                        ScrollAxis::Vertical,
                        self.scroll_state.clone(),
                    )
                    .child(div().px(px(20.0)).py(px(12.0)).child(content)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn home_renders_scrollable_shelves_and_survives_resize(cx: &mut TestAppContext) {
        let (home, cx) = cx.add_window_view(|_, cx| {
            let mut home = HomeScreen::new(cx);
            let item = HomeItem {
                uri: "spotify:playlist:example".into(),
                title: "Example playlist".into(),
                subtitle: "Example description".into(),
                kind: "Playlist".into(),
                playable: true,
                image: None,
            };
            home.feed.shortcuts = vec![item.clone(); 8];
            home.feed.shelves = vec![
                HomeShelf {
                    title: "Recommendations".into(),
                    items: vec![item.clone(); 10],
                    ..Default::default()
                };
                3
            ];
            home.feed.panels = vec![HomePanel {
                label: "Made for you".into(),
                item,
                video_url: None,
                poster: None,
            }];
            home.shelf_scrolls = (0..3).map(|_| ScrollHandle::new()).collect();
            home.shelf_states = (0..3)
                .map(|_| SmoothScrollState::new(cx.entity_id()))
                .collect();
            home
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.read_with(cx, |home, _| {
            assert!(home.scroll.max_offset().height > px(0.0));
            assert!(home.shelf_scrolls[0].max_offset().width > px(0.0));
        });
        cx.simulate_resize(gpui::size(px(500.0), px(700.0)));
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.update(cx, |home, cx| {
            home.expanded = Some(0);
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.read_with(cx, |home, _| {
            assert!(home.scroll.max_offset().height > px(0.0))
        });
    }
}
