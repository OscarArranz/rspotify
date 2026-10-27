//! Native library panel; UI state stays in this entity, network work stays on workers.

mod menus;
mod model;

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use futures::{StreamExt, channel::mpsc};
use gpui::{
    AnyElement, AppContext, Bounds, Context, Corner, Entity, EventEmitter, FocusHandle,
    HighlightStyle, InteractiveElement, IntoElement, KeyDownEvent, ListAlignment, ListState,
    ParentElement, Pixels, Render, ScrollHandle, StatefulInteractiveElement, Styled, StyledText,
    Subscription, Window, anchored, canvas, deferred, div, img, linear_color_stop, linear_gradient,
    list, point, prelude::FluentBuilder, px, rgb, rgba, svg,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::ContextMenuExt,
    scroll::{Scrollbar, ScrollbarShow},
};

use crate::lib::spotify::library::{
    LibraryEntry, LibraryHydrator, LibraryKind, LibrarySort, visible_entries,
};
use crate::lib::spotify::library_catalog::LibrarySupplement;
use crate::lib::spotify::{NativeSpotify, PlaybackSnapshot, PlaylistSource};
use crate::router::StackRouterHandle;
use crate::view::components::smooth_scroll::{SmoothScrollState, list_scroll_overlay};
use crate::view::screens::playlist::PlaylistScreen;
use model::{Filter, LibraryView, Ownership, match_ranges};

const BG: u32 = 0x191919;
const MUTED: u32 = 0xb3b3b3;
const GREEN: u32 = 0x1ed760;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibrarySize {
    Normal,
    Collapsed,
    Expanded,
}

enum LibraryUpdate {
    Loaded(Result<Vec<LibraryEntry>, String>),
    Hydrated(Vec<LibraryEntry>, usize),
    Catalog(LibrarySupplement, u64),
    Complete,
}

pub(crate) struct LibrarySidebar {
    client: Option<NativeSpotify>,
    entries: Vec<LibraryEntry>,
    visible: Vec<usize>,
    loading: bool,
    catalog_loading: bool,
    refreshing: bool,
    error: Option<String>,
    metadata_failures: usize,
    generation: u64,
    filter: Option<Filter>,
    ownership: Ownership,
    loaded_categories: HashSet<LibraryKind>,
    pin_set: Option<HashSet<String>>,
    pin_epoch: u64,
    pin_busy: bool,
    sort: LibrarySort,
    reverse_sort: bool,
    sort_explicit: bool,
    recents_supported: bool,
    view: LibraryView,
    folder: Option<String>,
    query: String,
    search: Entity<InputState>,
    search_open: bool,
    size: LibrarySize,
    last_open_size: LibrarySize,
    width: f32,
    chips: ScrollHandle,
    chip_edges: (bool, bool),
    list: ListState,
    list_scroll: SmoothScrollState,
    chip_scroll: SmoothScrollState,
    playback: PlaybackSnapshot,
    keyboard_index: usize,
    heading_hovered: bool,
    hovered_entry: Option<usize>,
    show_hover_card: bool,
    row_bounds: HashMap<usize, Bounds<Pixels>>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<LibrarySize> for LibrarySidebar {}

impl LibrarySidebar {
    pub fn new(client: Option<NativeSpotify>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search in Your Library"));
        let subscription = cx.subscribe(&search, |this, input, event: &InputEvent, cx| {
            match event {
                InputEvent::Change => {
                    this.query = input.read(cx).value().to_string();
                    this.rebuild();
                }
                // The toolbar returns to its compact resting state when an empty search loses focus.
                InputEvent::Blur if this.query.is_empty() => this.search_open = false,
                _ => return,
            }
            cx.notify();
        });
        let mut subscriptions = vec![subscription];
        if let Some(router) = cx.try_global::<StackRouterHandle>() {
            subscriptions.push(cx.observe(&router.entity(), |_, _, cx| cx.notify()));
        }
        let mut this = Self {
            playback: client
                .as_ref()
                .map(NativeSpotify::playback)
                .unwrap_or_default(),
            client,
            entries: Vec::new(),
            visible: Vec::new(),
            loading: true,
            catalog_loading: true,
            refreshing: false,
            error: None,
            metadata_failures: 0,
            generation: 0,
            filter: None,
            ownership: Ownership::All,
            loaded_categories: HashSet::new(),
            pin_set: None,
            pin_epoch: 0,
            pin_busy: false,
            sort: LibrarySort::Custom,
            reverse_sort: false,
            sort_explicit: false,
            recents_supported: false,
            view: LibraryView::List,
            folder: None,
            query: String::new(),
            search,
            search_open: false,
            size: LibrarySize::Normal,
            last_open_size: LibrarySize::Normal,
            width: 308.0,
            chips: ScrollHandle::new(),
            chip_edges: (false, false),
            list: ListState::new(1, ListAlignment::Top, px(256.0)),
            list_scroll: SmoothScrollState::new(cx.entity_id()),
            chip_scroll: SmoothScrollState::new(cx.entity_id()),
            keyboard_index: 0,
            heading_hovered: false,
            hovered_entry: None,
            show_hover_card: false,
            row_bounds: HashMap::new(),
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        let weak = cx.entity().downgrade();
        this.list.set_scroll_handler(move |_, _, cx| {
            let _ = weak.update(cx, |this, cx| {
                if this.hovered_entry.take().is_some() || this.show_hover_card {
                    this.show_hover_card = false;
                    cx.notify();
                }
            });
        });
        this.refresh(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        if let Some(client) = &this.client {
                            let snapshot = client.playback();
                            if this.playback.context_uri != snapshot.context_uri
                                || this.playback.is_playing != snapshot.is_playing
                            {
                                this.playback = snapshot;
                                cx.notify();
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        this
    }

    /// Optimistic startup creates this entity before the authenticated handle is ready.
    pub(crate) fn attach_client(&mut self, client: NativeSpotify, cx: &mut Context<Self>) {
        if self.client.is_some() {
            return;
        }
        self.playback = client.playback();
        self.client = Some(client);
        self.refresh(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.refreshing {
            return;
        }
        let Some(client) = self.client.clone() else {
            let restoring = cx
                .try_global::<crate::state::AppState>()
                .is_some_and(|state| state.is_authenticated());
            self.loading = restoring;
            self.error = (!restoring).then(|| "Sign in to load Your Library.".into());
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let pin_epoch = self.pin_epoch;
        self.refreshing = true;
        self.catalog_loading = true;
        self.loading = self.entries.is_empty();
        self.error = None;
        self.metadata_failures = 0;
        let (tx, mut rx) = mpsc::unbounded();
        std::thread::spawn(move || {
            match client.library_entries() {
                Ok(entries) => {
                    if tx
                        .unbounded_send(LibraryUpdate::Loaded(Ok(entries.clone())))
                        .is_err()
                    {
                        return;
                    }
                    if tx
                        .unbounded_send(LibraryUpdate::Catalog(client.library_catalog(), pin_epoch))
                        .is_err()
                    {
                        return;
                    }
                    let mut entries = entries;
                    entries.sort_by(|a, b| {
                        (!a.pin_priority())
                            .cmp(&(!b.pin_priority()))
                            .then_with(|| b.last_played_at.cmp(&a.last_played_at))
                    });
                    let mut cache = LibraryHydrator::default();
                    for entry in entries {
                        if tx.is_closed() {
                            return;
                        }
                        let mut batch = vec![entry];
                        let failures = client.hydrate_library_entries(&mut batch, &mut cache);
                        if tx
                            .unbounded_send(LibraryUpdate::Hydrated(batch, failures))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                Err(error) => {
                    let _ = tx.unbounded_send(LibraryUpdate::Loaded(Err(error.to_string())));
                }
            }
            let _ = tx.unbounded_send(LibraryUpdate::Complete);
        });
        cx.spawn(async move |this, cx| {
            while let Some(update) = rx.next().await {
                if this
                    .update(cx, |this, cx| {
                        if this.generation != generation {
                            return;
                        }
                        match update {
                            LibraryUpdate::Loaded(Ok(entries)) => {
                                let first_load = this.entries.is_empty();
                                this.entries = entries;
                                this.loading = false;
                                this.loaded_categories.clear();
                                if first_load && !this.sort_explicit && this.has_recents() {
                                    this.sort = LibrarySort::Recents;
                                }
                                if this
                                    .folder
                                    .as_ref()
                                    .is_some_and(|uri| !this.entries.iter().any(|e| &e.uri == uri))
                                {
                                    this.folder = None;
                                }
                                this.rebuild();
                            }
                            LibraryUpdate::Loaded(Err(error)) => {
                                this.loading = false;
                                this.catalog_loading = false;
                                this.error = Some(error);
                            }
                            LibraryUpdate::Catalog(mut catalog, request_pin_epoch) => {
                                this.recents_supported = catalog.recents_available;
                                let offset = this.list.logical_scroll_top();
                                let anchor_row =
                                    offset.item_ix.saturating_sub(this.toolbar_offset())
                                        * this.columns();
                                let anchor = this
                                    .visible
                                    .get(anchor_row)
                                    .map(|&index| this.entries[index].uri.clone());
                                if request_pin_epoch == this.pin_epoch {
                                    if catalog.pins.is_some() {
                                        this.pin_set = catalog.pins.clone();
                                    }
                                } else {
                                    catalog.pins = this.pin_set.clone();
                                }
                                let (categories, failures) = catalog.apply(&mut this.entries);
                                this.loaded_categories = categories;
                                this.metadata_failures += failures;
                                this.catalog_loading = false;
                                if !this.sort_explicit && this.has_recents() {
                                    this.sort = LibrarySort::Recents;
                                }
                                this.rebuild();
                                if offset.item_ix > 0 {
                                    if let Some(position) = anchor.and_then(|uri| {
                                        this.visible
                                            .iter()
                                            .position(|&index| this.entries[index].uri == uri)
                                    }) {
                                        this.list.scroll_to(gpui::ListOffset {
                                            item_ix: this.toolbar_offset()
                                                + position / this.columns(),
                                            offset_in_item: offset.offset_in_item,
                                        });
                                    }
                                }
                            }
                            LibraryUpdate::Hydrated(entries, _failures) => {
                                for entry in entries {
                                    if let Some(old) =
                                        this.entries.iter_mut().find(|e| e.uri == entry.uri)
                                    {
                                        let pinned = old.pinned;
                                        *old = entry;
                                        old.pinned = pinned;
                                    }
                                }
                                // Per-item hydration misses (for example a playlist whose
                                // metadata is 403/404 for this account) keep the rootlist name and
                                // owner, so they are not a panel-wide failure. Only catalog-level
                                // failures raise the retry banner.
                                let visible = this.filtered();
                                if visible != this.visible {
                                    this.visible = visible;
                                    // Profile hydration can change an owner search. Only reset if row count changed.
                                    if this.list.item_count() != this.list_count() {
                                        this.list.reset(this.list_count());
                                    }
                                }
                            }
                            LibraryUpdate::Complete => this.refreshing = false,
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn has_recents(&self) -> bool {
        self.recents_supported || self.entries.iter().any(|e| e.last_played_at.is_some())
    }
    fn filter_available(&self, filter: Filter) -> bool {
        filter == Filter::Playlists || self.loaded_categories.contains(&filter.kind())
    }
    fn toolbar_offset(&self) -> usize {
        usize::from(self.size == LibrarySize::Normal)
    }
    fn columns(&self) -> usize {
        if self.size == LibrarySize::Collapsed {
            1
        } else {
            self.view.columns(self.width)
        }
    }
    fn list_count(&self) -> usize {
        self.toolbar_offset() + self.visible.len().div_ceil(self.columns())
    }

    fn filtered(&self) -> Vec<usize> {
        if self.filter.is_some_and(|f| !self.filter_available(f)) {
            return Vec::new();
        }
        let username = self
            .client
            .as_ref()
            .map(NativeSpotify::username)
            .unwrap_or("");
        let mut visible = visible_entries(
            &self.entries,
            &self.query,
            self.folder.as_deref(),
            self.sort,
        );
        if let Some(filter) = self.filter {
            visible.retain(|&index| filter.matches(self.entries[index].kind));
        }
        if self.filter == Some(Filter::Playlists) {
            visible.retain(|&index| {
                self.ownership
                    .keeps_in_playlists(&self.entries[index], username)
            });
        }
        if self.reverse_sort {
            // Keep the collection at the top, independent of the direction of normal entries.
            let start = visible
                .iter()
                .take_while(|&&i| self.entries[i].pin_priority())
                .count();
            visible[start..].reverse();
        }
        visible
    }

    fn rebuild(&mut self) {
        self.list_scroll.cancel();
        self.visible = self.filtered();
        self.keyboard_index = 0;
        self.hovered_entry = None;
        self.show_hover_card = false;
        self.row_bounds.clear();
        self.list.reset(self.list_count());
    }

    fn change_size(&mut self, size: LibrarySize, cx: &mut Context<Self>) {
        if size == LibrarySize::Collapsed {
            self.last_open_size = self.size;
        }
        self.size = size;
        self.rebuild();
        cx.emit(size);
        cx.notify();
    }

    fn select_filter(&mut self, filter: Option<Filter>, cx: &mut Context<Self>) {
        self.filter = filter;
        self.ownership = Ownership::All;
        self.chip_scroll.cancel();
        self.chips.set_offset(point(px(0.0), px(0.0)));
        self.rebuild();
        cx.notify();
    }

    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = true;
        self.search.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = false;
        self.query.clear();
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.focus.focus(window);
        self.rebuild();
        cx.notify();
    }

    fn open(&mut self, entry: LibraryEntry, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window);
        if let Some(position) = self
            .visible
            .iter()
            .position(|&index| self.entries[index].uri == entry.uri)
        {
            self.keyboard_index = position;
        }
        if entry.kind == LibraryKind::Folder {
            self.folder = Some(entry.uri);
            self.rebuild();
            cx.notify();
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(router) = cx.try_global::<StackRouterHandle>().cloned() else {
            return;
        };
        let source = if entry.kind == LibraryKind::LikedSongs {
            PlaylistSource::LikedSongs
        } else if matches!(
            entry.kind,
            LibraryKind::Album | LibraryKind::Artist | LibraryKind::Show
        ) {
            PlaylistSource::Context {
                uri: entry.uri.clone(),
                name: entry.name,
                owner: entry.owner,
                image_url: entry.image_url,
                kind: entry.kind.label(),
            }
        } else {
            PlaylistSource::Playlist {
                uri: entry.uri.clone(),
                name: entry.name,
                owner: entry.owner,
                image_url: entry.image_url.or_else(|| entry.covers.first().cloned()),
            }
        };
        if router.current_source(cx) != Some(entry.uri.as_str()) {
            let screen = cx.new(|cx| PlaylistScreen::new(source, client, cx));
            router.push_for_source(screen.into(), entry.uri, cx);
        }
    }

    fn play(&mut self, uri: &str, cx: &mut Context<Self>) {
        if let Some(client) = &self.client {
            let result = if self.playback.context_uri.as_deref() == Some(uri) {
                client.play_pause()
            } else {
                client.play_context(uri)
            };
            if let Err(error) = result {
                self.error = Some(error.to_string());
            }
            cx.notify();
        }
    }

    fn toggle_pin(&mut self, entry: &LibraryEntry, cx: &mut Context<Self>) {
        if self.pin_busy || entry.pinned.is_none() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let pinned = entry.pinned != Some(true);
        if pinned && entry.parent.is_some() {
            self.error = Some("Move this item out of its folder before pinning it.".into());
            cx.notify();
            return;
        }
        // Match the installed desktop client's default, not a claimed universal backend limit.
        if pinned && self.pin_set.as_ref().is_some_and(|pins| pins.len() >= 4) {
            self.error = Some("The desktop default allows 4 pins. Unpin an item first.".into());
            cx.notify();
            return;
        }
        let uri = entry.pin_uri();
        self.pin_busy = true;
        self.pin_epoch = self.pin_epoch.wrapping_add(1);
        self.error = None;
        let (tx, rx) = futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(client.set_library_pinned(&uri, pinned));
        });
        cx.spawn(async move |this, cx| {
            let result = rx.await;
            let _ = this.update(cx, |this, cx| {
                this.pin_busy = false;
                this.pin_epoch = this.pin_epoch.wrapping_add(1);
                match result {
                    Ok(Ok(pins)) => {
                        crate::lib::spotify::library_catalog::apply_pins(&mut this.entries, &pins);
                        this.pin_set = Some(pins);
                        this.rebuild();
                    }
                    Ok(Err(error)) => this.error = Some(error.to_string()),
                    Err(_) => {
                        this.error = Some("Pin request failed; refresh to check its result.".into())
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn render_row(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let entry = self.entries[index].clone();
        let active = self.playback.context_uri.as_deref() == Some(entry.uri.as_str());
        let selected = cx
            .try_global::<StackRouterHandle>()
            .and_then(|r| r.current_source(cx))
            == Some(entry.uri.as_str());
        let collapsed = self.size == LibrarySize::Collapsed;
        let compact = self.view == LibraryView::Compact && !collapsed;
        let grid = self.view.is_grid() && !collapsed;
        let labels = !collapsed && self.view != LibraryView::CompactGrid;
        let padding = if grid && self.view == LibraryView::Grid {
            12.0
        } else {
            8.0
        };
        let cell_width = (self.width - 16.0) / self.columns() as f32;
        let image_size = if grid {
            (cell_width - padding * 2.0).max(48.0)
        } else {
            48.0
        };
        let click_entry = entry.clone();
        let menu_entry = entry.clone();
        let play_uri = entry.uri.clone();
        let weak = cx.entity().downgrade();
        let context_weak = weak.clone();
        let keyboard_selected =
            self.focus.is_focused(window) && self.visible.get(self.keyboard_index) == Some(&index);
        let playing = active && self.playback.is_playing;

        div()
            .id(gpui::SharedString::from(entry.uri.clone()))
            .group("library-row")
            .relative()
            .flex()
            .items_center()
            .when(grid, |d| {
                d.flex_col().items_start().flex_none().w(px(cell_width))
            })
            .when(!grid, |d| {
                d.w_full().h(px(if compact { 32.0 } else { 64.0 }))
            })
            .min_w(px(0.0))
            .p(px(padding))
            .gap(px(if grid { 8.0 } else { 12.0 }))
            .rounded(px(4.0))
            .when(selected, |d| d.bg(rgb(0x303030)))
            .when(keyboard_selected && !selected, |d| d.bg(rgb(0x242424)))
            .hover(|s| s.bg(rgb(if selected { 0x393939 } else { 0x242424 })))
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _, window, cx| this.open(click_entry.clone(), window, cx)),
            )
            .on_hover(cx.listener(move |this, hover: &bool, _, cx| {
                this.hovered_entry = hover.then_some(index);
                this.show_hover_card = false;
                if *hover && this.size == LibrarySize::Collapsed {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(350))
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            if this.hovered_entry == Some(index) {
                                this.show_hover_card = true;
                                cx.notify();
                            }
                        });
                    })
                    .detach();
                }
                cx.notify();
            }))
            .when(!compact, |d| {
                d.child(
                    div()
                        .relative()
                        .size(px(image_size))
                        .flex_shrink_0()
                        .child(cover(&entry, image_size))
                        .when(!collapsed && entry.kind != LibraryKind::Folder, |d| {
                            d.child(
                                div()
                                    .id(("library-artwork-play", index))
                                    .absolute()
                                    .inset_0()
                                    .rounded(px(4.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(rgba(0x00000066))
                                    .opacity(0.0)
                                    .group_hover("library-row", |s| s.opacity(1.0))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.play(&play_uri, cx);
                                    }))
                                    .child(icon(
                                        if playing {
                                            "icons/pause.svg"
                                        } else {
                                            "icons/play.svg"
                                        },
                                        if grid { 32.0 } else { 16.0 },
                                        0xffffff,
                                    )),
                            )
                        }),
                )
            })
            .when(compact, |d| {
                d.child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(4.0))
                        .min_w(px(0.0))
                        .flex_1()
                        .overflow_hidden()
                        .when(entry.pinned == Some(true), |d| {
                            d.child(icon("icons/pin.svg", 16.0, GREEN))
                        })
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(16.0))
                                .text_color(rgb(if active { GREEN } else { 0xffffff }))
                                .child(highlight(&entry.name, &self.query)),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(14.0))
                                .text_color(rgb(MUTED))
                                .child("•"),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(14.0))
                                .text_color(rgb(MUTED))
                                .child(entry.type_label.clone()),
                        ),
                )
            })
            .when(labels && !compact, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .when(!grid, |d| d.flex_1())
                        .min_w(px(0.0))
                        .w_full()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(16.0))
                                .line_height(px(24.0))
                                .text_color(rgb(if active { GREEN } else { 0xffffff }))
                                .child(highlight(&entry.name, &self.query)),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .min_w(px(0.0))
                                .when(entry.pinned == Some(true), |d| {
                                    d.child(icon("icons/pin.svg", 12.0, GREEN))
                                })
                                .when(entry.type_label == "Upcoming album", |d| {
                                    d.child(icon("icons/clock.svg", 12.0, MUTED))
                                })
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(14.0))
                                        .line_height(px(20.0))
                                        .text_color(rgb(MUTED))
                                        .child(highlight(&entry.subtitle(), &self.query)),
                                ),
                        ),
                )
            })
            .when(playing && !collapsed && !grid, |d| {
                d.child(icon("icons/volume-high.svg", 16.0, GREEN))
            })
            .when(compact && entry.type_label == "Upcoming album", |d| {
                d.child(icon("icons/clock.svg", 16.0, MUTED))
            })
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, _| {
                            this.row_bounds.insert(index, bounds);
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .context_menu(move |menu, window, cx| {
                Self::playlist_menu(menu, menu_entry.clone(), context_weak.clone(), window, cx)
            })
            .into_any_element()
    }

    fn hover_card(&self) -> Option<AnyElement> {
        if self.size != LibrarySize::Collapsed || !self.show_hover_card {
            return None;
        }
        let index = self.hovered_entry?;
        let entry = self.entries.get(index)?;
        let bounds = self.row_bounds.get(&index)?;
        let active = self.playback.context_uri.as_deref() == Some(entry.uri.as_str());
        let playing = active && self.playback.is_playing;
        Some(
            deferred(
                anchored()
                    .anchor(Corner::TopLeft)
                    .position(point(bounds.right() + px(8.0), bounds.top() + px(8.0)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(20.0))
                            .p(px(8.0))
                            .rounded(px(4.0))
                            .bg(rgb(0x282828))
                            .shadow_lg()
                            .font_family("Spotify Mix UI")
                            .max_w(px(400.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .text_size(px(16.0))
                                            .line_height(px(24.0))
                                            .text_color(rgb(if active { GREEN } else { 0xffffff }))
                                            .child(entry.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .text_size(px(14.0))
                                            .line_height(px(20.0))
                                            .text_color(rgb(MUTED))
                                            .when(entry.pinned == Some(true), |d| {
                                                d.child(icon("icons/pin.svg", 12.0, GREEN))
                                            })
                                            .when(entry.type_label == "Upcoming album", |d| {
                                                d.child(icon("icons/clock.svg", 12.0, MUTED))
                                            })
                                            .child(entry.subtitle()),
                                    ),
                            )
                            .when(playing, |d| {
                                d.child(icon("icons/volume-high.svg", 16.0, GREEN))
                            }),
                    ),
            )
            .into_any_element(),
        )
    }
}

impl Render for LibrarySidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = self.size == LibrarySize::Collapsed;
        let expanded = self.size == LibrarySize::Expanded;
        let weak = cx.entity().downgrade();
        let scroll_weak = weak.clone();
        let columns = self.columns();
        let toolbar_offset = self.toolbar_offset();
        div()
            .id("library-panel")
            .track_focus(&self.focus)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .min_w(px(0.0))
            .bg(rgb(BG))
            .font_family("Spotify Mix UI")
            .rounded(px(8.0))
            .overflow_hidden()
            .text_color(rgb(0xffffff))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && this.search_open {
                    this.close_search(window, cx);
                    cx.stop_propagation();
                    return;
                }
                if !this.focus.is_focused(window) {
                    return;
                }
                if event.keystroke.key == "space"
                    && !event.keystroke.modifiers.modified()
                    && !event.is_held
                {
                    if let Some(client) = &this.client {
                        if let Err(error) = client.play_pause() {
                            this.error = Some(error.to_string());
                            cx.notify();
                        }
                    }
                    cx.stop_propagation();
                    return;
                }
                if this.visible.is_empty() {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" => this.keyboard_index = this.keyboard_index.saturating_sub(1),
                    "down" => {
                        this.keyboard_index = (this.keyboard_index + 1).min(this.visible.len() - 1)
                    }
                    "enter" => {
                        if let Some(&index) = this.visible.get(this.keyboard_index) {
                            this.open(this.entries[index].clone(), window, cx);
                        }
                    }
                    _ => return,
                }
                this.list_scroll.cancel();
                this.list.scroll_to_reveal_item(
                    this.toolbar_offset() + this.keyboard_index / this.columns(),
                );
                cx.stop_propagation();
                cx.notify();
            }))
            .child(self.render_header(cx))
            .when(!collapsed && !expanded, |d| d.child(self.render_chips(cx)))
            .when(expanded, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(48.0))
                        .flex_shrink_0()
                        .child(div().flex_1().min_w(px(0.0)).child(self.render_chips(cx)))
                        .child(div().w(px(400.0)).child(self.render_toolbar(cx))),
                )
            })
            .when(collapsed, |d| {
                d.child(
                    div()
                        .h(px(48.0))
                        .flex_shrink_0()
                        .flex()
                        .justify_center()
                        .child(self.render_add_button(cx)),
                )
            })
            .when(self.folder.is_some() && !collapsed, |d| {
                d.child(
                    Button::new("library-parent")
                        .ghost()
                        .mx(px(8.0))
                        .justify_start()
                        .child(icon("icons/back.svg", 16.0, MUTED))
                        .label(
                            self.entries
                                .iter()
                                .find(|e| Some(&e.uri) == self.folder.as_ref())
                                .map(|e| e.name.clone())
                                .unwrap_or_else(|| "Your Library".into()),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.folder = this
                                .entries
                                .iter()
                                .find(|e| Some(&e.uri) == this.folder.as_ref())
                                .and_then(|e| e.parent.clone());
                            this.rebuild();
                            cx.notify();
                        })),
                )
            })
            .when(self.loading && !collapsed, |d| {
                d.child(message("Loading Your Library…"))
            })
            .when(self.error.is_some() && !collapsed, |d| {
                d.child(
                    div()
                        .px(px(16.0))
                        .py(px(8.0))
                        .text_size(px(12.0))
                        .text_color(rgb(MUTED))
                        .child(self.error.clone().unwrap_or_default())
                        .child(
                            Button::new("library-retry")
                                .ghost()
                                .label(if self.refreshing {
                                    "Refreshing…"
                                } else {
                                    "Retry"
                                })
                                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                        ),
                )
            })
            .when(
                self.catalog_loading
                    && self.filter.is_some_and(|f| f != Filter::Playlists)
                    && !collapsed,
                |d| d.child(message("Loading saved library items…")),
            )
            .when(
                !self.catalog_loading
                    && self
                        .filter
                        .is_some_and(|filter| !self.filter_available(filter))
                    && !collapsed,
                |d| {
                    d.child(message(
                        "This category could not be loaded from Spotify. Refresh to retry.",
                    ))
                },
            )
            .when(
                !self.loading
                    && !self.catalog_loading
                    && self.visible.is_empty()
                    && self.error.is_none()
                    && !self.filter.is_some_and(|f| !self.filter_available(f))
                    && !collapsed,
                |d| {
                    d.child(message(if self.query.is_empty() {
                        "Nothing here yet"
                    } else {
                        "No results in Your Library"
                    }))
                },
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(
                        list(
                            self.list.clone(),
                            cx.processor(move |this, row: usize, window, cx| {
                                if toolbar_offset == 1 && row == 0 {
                                    return this.render_toolbar(cx);
                                }
                                let start = row.saturating_sub(toolbar_offset) * columns;
                                div()
                                    .flex()
                                    .w_full()
                                    .min_w(px(0.0))
                                    .children(
                                        this.visible
                                            .iter()
                                            .skip(start)
                                            .take(columns)
                                            .copied()
                                            .map(|index| this.render_row(index, window, cx)),
                                    )
                                    .into_any_element()
                            }),
                        )
                        .flex_1()
                        .min_h(px(0.0))
                        .mx(px(if collapsed { 4.0 } else { 8.0 }))
                        .mb(px(8.0)),
                    )
                    .child(list_scroll_overlay(
                        self.list.clone(),
                        self.list_scroll.clone(),
                        move |_, cx| {
                            let _ = scroll_weak.update(cx, |this, _| {
                                this.hovered_entry = None;
                                this.show_hover_card = false;
                            });
                        },
                    ))
                    .child(Scrollbar::vertical(&self.list).scrollbar_show(ScrollbarShow::Hover)),
            )
            .when(self.metadata_failures > 0 && !collapsed, |d| {
                d.child(message(
                    "Some library metadata could not be loaded. Refresh to retry.",
                ))
            })
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            let offset = this.chips.offset().x;
                            let edges = (
                                offset < px(-1.0),
                                -offset < this.chips.max_offset().width - px(1.0),
                            );
                            if edges != this.chip_edges {
                                this.chip_edges = edges;
                                cx.notify();
                            }
                            let width = f32::from(bounds.size.width);
                            if (this.width - width).abs() > 1.0 {
                                let columns = this.columns();
                                this.width = width;
                                if this.view.is_grid() && this.size != LibrarySize::Collapsed {
                                    let offset = this.list.logical_scroll_top();
                                    this.list.reset(this.list_count());
                                    if columns == this.columns() {
                                        this.list.scroll_to(offset);
                                    }
                                }
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(self.hover_card())
    }
}

fn highlight(text: &str, query: &str) -> StyledText {
    StyledText::new(text.to_string()).with_highlights(match_ranges(text, query).into_iter().map(
        |range| {
            (
                range,
                HighlightStyle {
                    color: Some(rgb(0xffffff).into()),
                    background_color: Some(rgb(0x2e77d0).into()),
                    ..Default::default()
                },
            )
        },
    ))
}

fn message(text: &str) -> AnyElement {
    div()
        .p(px(16.0))
        .text_size(px(14.0))
        .text_color(rgb(MUTED))
        .child(text.to_string())
        .into_any_element()
}

fn icon(path: &'static str, size: f32, color: u32) -> AnyElement {
    svg()
        .path(path)
        .size(px(size))
        .flex_shrink_0()
        .text_color(rgb(color))
        .into_any_element()
}

fn cover(entry: &LibraryEntry, size: f32) -> AnyElement {
    if let Some(url) = &entry.image_url {
        return img(url.clone())
            .size(px(size))
            .flex_shrink_0()
            .rounded(px(4.0))
            .when(entry.kind == LibraryKind::Artist, |i| i.rounded_full())
            .into_any_element();
    }
    if entry.covers.len() >= 4 {
        return div()
            .size(px(size))
            .flex_shrink_0()
            .rounded(px(4.0))
            .overflow_hidden()
            .flex()
            .flex_col()
            .children(entry.covers[..4].chunks(2).map(|row| {
                div()
                    .flex()
                    .children(row.iter().map(|url| img(url.clone()).size(px(size / 2.0))))
            }))
            .into_any_element();
    }
    if let Some(url) = entry.covers.first() {
        return img(url.clone())
            .size(px(size))
            .flex_shrink_0()
            .rounded(px(4.0))
            .into_any_element();
    }
    let liked = entry.kind == LibraryKind::LikedSongs;
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(if liked {
            linear_gradient(
                135.0,
                linear_color_stop(rgb(0x450af5), 0.0),
                linear_color_stop(rgb(0xc4efd9), 1.0),
            )
        } else {
            linear_gradient(
                135.0,
                linear_color_stop(rgb(0x333333), 0.0),
                linear_color_stop(rgb(0x242424), 1.0),
            )
        })
        .child(icon(
            if liked {
                "icons/heart.svg"
            } else if entry.kind == LibraryKind::Folder {
                "icons/folder.svg"
            } else {
                "icons/list.svg"
            },
            size * 0.42,
            if liked { 0xffffff } else { MUTED },
        ))
        .into_any_element()
}
