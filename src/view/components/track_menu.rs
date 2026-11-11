//! The shared track popup. Native GPUI hit testing, anchored layout and focus are
//! used directly because PopupMenu fixes submenu rows at 26px and cannot express
//! the reference's uniform 40px rows without changing unrelated library menus.
use std::{cell::RefCell, collections::HashSet, rc::Rc};

use crate::{
    lib::spotify::{
        LikedStatus, NativeSpotify, PlaylistOccurrence, PlaylistSource, SleepTimerChoice,
        TrackCredits,
    },
    router::StackRouterHandle,
    state::AppState,
    view::screens::{playlist::PlaylistScreen, search::open_artist_profile},
};
use futures::channel::oneshot;
use gpui::{
    AnyElement, App, AppContext, Context, Edges, Entity, FocusHandle, Global, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, ParentElement, Pixels, Point, Render, SharedString,
    StatefulInteractiveElement, Styled, WeakEntity, Window, anchored, canvas, deferred, div, point,
    prelude::FluentBuilder, px, rgb, rgba, svg,
};

/// Captured by value, never resolved later through a virtual row index.
#[derive(Clone)]
pub(crate) struct TrackMenuTarget {
    pub uri: String,
    pub title: String,
    pub occurrence: Option<PlaylistOccurrence>,
}
impl TrackMenuTarget {
    pub(crate) fn new(uri: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            title: title.into(),
            occurrence: None,
        }
    }
}

type Removed = Rc<dyn Fn(&mut App)>;
type Handler = Rc<dyn Fn(&mut Window, &mut App)>;
type Dispatch = Rc<dyn Fn(Command, &mut Window, &mut App)>;
type Entries = Rc<RefCell<Vec<Entry>>>;

#[derive(Clone)]
struct TrackMenuHandle(Entity<TrackMenuHost>);
impl Global for TrackMenuHandle {}

pub(crate) struct TrackMenuHost {
    menu: Option<OpenMenu>,
    message: Option<(String, bool)>,
    credits: Option<TrackCredits>,
    credits_loading: bool,
    credits_generation: u64,
    credits_previous_focus: Option<FocusHandle>,
    pending: HashSet<String>,
    focus: FocusHandle,
}
struct OpenMenu {
    entries: Vec<Entry>,
    position: Point<Pixels>,
    selected: usize,
    submenu_open: bool,
    submenu_selected: Option<usize>,
    previous_focus: Option<FocusHandle>,
    scope: SharedString,
    root_scroll: gpui::ScrollHandle,
    submenu_scroll: gpui::ScrollHandle,
}
#[derive(Clone)]
enum Entry {
    Action {
        label: String,
        icon: &'static str,
        enabled: bool,
        checked: bool,
        dismiss: bool,
        handler: Handler,
    },
    Submenu {
        label: &'static str,
        icon: &'static str,
        entries: Entries,
    },
    Separator,
}
impl Entry {
    fn enabled(&self) -> bool {
        matches!(
            self,
            Self::Action { enabled: true, .. } | Self::Submenu { .. }
        )
    }
}
#[derive(Clone)]
enum Command {
    Like,
    Queue,
    Remove,
    Add(String),
    Radio,
    Artist,
    Album,
    Credits,
    Timer(SleepTimerChoice),
}
struct MenuSession {
    target: TrackMenuTarget,
    client: NativeSpotify,
    host: WeakEntity<TrackMenuHost>,
    removed: Option<Removed>,
}

pub(crate) fn init(cx: &mut App) -> Entity<TrackMenuHost> {
    if let Some(handle) = cx.try_global::<TrackMenuHandle>() {
        return handle.0.clone();
    }
    let host = cx.new(|cx| TrackMenuHost {
        menu: None,
        message: None,
        credits: None,
        credits_loading: false,
        credits_generation: 0,
        credits_previous_focus: None,
        pending: HashSet::new(),
        focus: cx.focus_handle(),
    });
    cx.set_global(TrackMenuHandle(host.clone()));
    host
}

/// The unique ancestor scopes sibling virtual targets. Ordinary GPUI mouse-down
/// hit testing respects clipping; there is no window-wide geometric row listener.
pub(crate) fn track_menu<E>(
    element: E,
    scope: impl Into<SharedString>,
    target: TrackMenuTarget,
    removed: Option<Removed>,
) -> AnyElement
where
    E: IntoElement + 'static,
{
    if !target.uri.starts_with("spotify:track:") {
        return element.into_any_element();
    }
    let scope = scope.into();
    target_surface(element, scope.clone(), move |position, window, cx| {
        let Some(client) = cx
            .try_global::<AppState>()
            .and_then(AppState::spotify_client)
        else {
            return;
        };
        let host = init(cx);
        client.ensure_liked_tracks_loaded();
        let liked = client.liked_status(&target.uri);
        let timer = client.sleep_timer();
        let session = Rc::new(MenuSession {
            target: target.clone(),
            client,
            host: host.downgrade(),
            removed: removed.clone(),
        });
        let dispatch_session = session.clone();
        let dispatch: Dispatch = Rc::new(move |command, window, cx| {
            execute(dispatch_session.clone(), command, window, cx)
        });
        let destinations = Rc::new(RefCell::new(Vec::new()));
        let entries = menu_entries(
            target.occurrence.is_some(),
            liked,
            timer,
            dispatch.clone(),
            destinations.clone(),
            &target.uri,
        );
        host.update(cx, |host, cx| {
            host.open(entries, scope.clone(), position, window, cx)
        });
        load_playlists(session, dispatch, destinations, cx);
    })
}

fn target_surface<E: IntoElement + 'static>(
    element: E,
    scope: SharedString,
    open: impl Fn(Point<Pixels>, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let highlight_scope = scope.clone();
    div()
        .id(scope)
        .relative()
        .flex_shrink_0()
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            cx.stop_propagation();
            open(event.position, window, cx);
        })
        .child(element)
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    let active = cx.try_global::<TrackMenuHandle>().is_some_and(|handle| {
                        handle
                            .0
                            .read(cx)
                            .menu
                            .as_ref()
                            .is_some_and(|menu| menu.scope == highlight_scope)
                    });
                    if active {
                        window.paint_quad(gpui::fill(bounds, rgba(0xffffff1a)));
                    }
                },
            )
            .absolute()
            .size_full(),
        )
        .into_any_element()
}

fn action(
    label: impl Into<String>,
    icon: &'static str,
    command: Command,
    dispatch: &Dispatch,
) -> Entry {
    let dispatch = dispatch.clone();
    Entry::Action {
        label: label.into(),
        icon,
        enabled: true,
        checked: false,
        dismiss: true,
        handler: Rc::new(move |window, cx| dispatch(command.clone(), window, cx)),
    }
}
fn unavailable(label: &str, icon: &'static str) -> Entry {
    Entry::Action {
        label: label.into(),
        icon,
        enabled: false,
        checked: false,
        dismiss: true,
        handler: Rc::new(|_, _| {}),
    }
}
fn status(label: impl Into<String>) -> Entry {
    Entry::Action {
        label: label.into(),
        icon: "icons/list.svg",
        enabled: false,
        checked: false,
        dismiss: true,
        handler: Rc::new(|_, _| {}),
    }
}
fn checked(mut entry: Entry, value: bool) -> Entry {
    if let Entry::Action { checked, .. } = &mut entry {
        *checked = value;
    }
    entry
}

fn menu_entries(
    removable: bool,
    liked: LikedStatus,
    timer: SleepTimerChoice,
    dispatch: Dispatch,
    destinations: Entries,
    uri: &str,
) -> Vec<Entry> {
    let mut entries = vec![Entry::Submenu {
        label: "Add to playlist",
        icon: "icons/plus.svg",
        entries: destinations,
    }];
    if removable {
        entries.push(action(
            "Remove from this playlist",
            "icons/minus.svg",
            Command::Remove,
            &dispatch,
        ));
    }
    entries.push(checked(
        action(
            match liked {
                LikedStatus::Liked => "Remove from your Liked Songs",
                LikedStatus::NotLiked => "Add to your Liked Songs",
                LikedStatus::Unknown => "Check Liked Songs status…",
            },
            if liked == LikedStatus::Liked {
                "icons/check.svg"
            } else {
                "icons/plus.svg"
            },
            Command::Like,
            &dispatch,
        ),
        liked == LikedStatus::Liked,
    ));
    entries.push(action(
        "Add to queue",
        "icons/queue-add.svg",
        Command::Queue,
        &dispatch,
    ));
    entries.push(unavailable(
        "Exclude from your taste profile",
        "icons/taste-exclude.svg",
    ));
    entries.push(unavailable("Start a Jam", "icons/friends-outlined.svg"));
    let mut timers = Vec::new();
    for minutes in [5, 10, 15, 30, 45, 60] {
        let choice = SleepTimerChoice::Minutes(minutes);
        timers.push(checked(
            action(
                format!("{minutes} minutes"),
                "icons/clock.svg",
                Command::Timer(choice),
                &dispatch,
            ),
            timer == choice,
        ));
    }
    timers.push(checked(
        action(
            "End of track",
            "icons/clock.svg",
            Command::Timer(SleepTimerChoice::EndOfTrack),
            &dispatch,
        ),
        timer == SleepTimerChoice::EndOfTrack,
    ));
    timers.push(Entry::Separator);
    timers.push(checked(
        action(
            "Turn off timer",
            "icons/close.svg",
            Command::Timer(SleepTimerChoice::Off),
            &dispatch,
        ),
        timer == SleepTimerChoice::Off,
    ));
    entries.extend([
        Entry::Submenu {
            label: "Sleep timer",
            icon: "icons/clock.svg",
            entries: Rc::new(RefCell::new(timers)),
        },
        Entry::Separator,
        action(
            "Go to song radio",
            "icons/radio.svg",
            Command::Radio,
            &dispatch,
        ),
        action(
            "Go to artist",
            "icons/artist.svg",
            Command::Artist,
            &dispatch,
        ),
        action("Go to album", "icons/album.svg", Command::Album, &dispatch),
        action(
            "View credits",
            "icons/credits.svg",
            Command::Credits,
            &dispatch,
        ),
    ]);
    let uri = uri.to_string();
    entries.push(Entry::Submenu {
        label: "Share",
        icon: "icons/share.svg",
        entries: Rc::new(RefCell::new(vec![Entry::Action {
            label: "Copy song link".into(),
            icon: "icons/share.svg",
            enabled: true,
            checked: false,
            dismiss: true,
            handler: Rc::new(move |_, cx| {
                if let Some(id) = uri.strip_prefix("spotify:track:") {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(format!(
                        "https://open.spotify.com/track/{id}"
                    )));
                }
            }),
        }])),
    });
    entries
}

fn load_playlists(
    session: Rc<MenuSession>,
    dispatch: Dispatch,
    destinations: Entries,
    cx: &mut App,
) {
    *destinations.borrow_mut() = vec![status("Loading editable playlists…")];
    let _ = session.host.update(cx, |_, cx| cx.notify());
    let client = session.client.clone();
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(client.editable_playlists());
    });
    cx.spawn(async move |cx| {
        let result = rx.await.map_err(|_| "Playlist worker stopped".to_string()).and_then(|result| result.map_err(|error| error.to_string()));
        let _ = session.host.update(cx, |host, cx| {
            *destinations.borrow_mut() = match result {
                Ok(playlists) if playlists.is_empty() => vec![status("No editable playlists")],
                Ok(playlists) => playlists.into_iter().map(|playlist| action(playlist.name, "icons/list.svg", Command::Add(playlist.uri), &dispatch)).collect(),
                Err(error) => {
                    let retry_session = session.clone();
                    let retry_dispatch = dispatch.clone();
                    let retry_destinations = Rc::downgrade(&destinations);
                    vec![status(format!("Couldn't load playlists: {error}")), Entry::Action {
                        label: "Retry loading playlists".into(), icon: "icons/forward.svg", enabled: true, checked: false, dismiss: false,
                        handler: Rc::new(move |_, cx| {
                            if let Some(destinations) = retry_destinations.upgrade() {
                                load_playlists(retry_session.clone(), retry_dispatch.clone(), destinations, cx);
                            }
                        }),
                    }]
                }
            };
            if let Some(menu) = &mut host.menu {
                if matches!(menu.entries.get(menu.selected), Some(Entry::Submenu { entries, .. }) if Rc::ptr_eq(entries, &destinations)) {
                    menu.submenu_selected = destinations.borrow().iter().position(Entry::enabled);
                }
            }
            cx.notify();
        });
    }).detach();
}

impl TrackMenuHost {
    fn open(
        &mut self,
        entries: Vec<Entry>,
        scope: SharedString,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = Some(OpenMenu {
            entries,
            scope,
            position,
            selected: 0,
            submenu_open: false,
            submenu_selected: None,
            previous_focus: window.focused(cx),
            root_scroll: gpui::ScrollHandle::new(),
            submenu_scroll: gpui::ScrollHandle::new(),
        });
        window.focus(&self.focus);
        window.refresh();
        cx.notify();
    }
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.menu.take() {
            if let Some(focus) = menu.previous_focus {
                window.focus(&focus);
            } else {
                window.blur();
            }
        }
        cx.notify();
        window.refresh();
    }
    fn select(&mut self, index: usize, submenu: bool, cx: &mut Context<Self>) {
        let Some(menu) = &mut self.menu else {
            return;
        };
        if submenu {
            menu.submenu_selected = Some(index);
        } else {
            menu.selected = index;
            menu.submenu_open = matches!(menu.entries.get(index), Some(Entry::Submenu { .. }));
            menu.submenu_selected = None;
        }
        cx.notify();
    }
    fn choose(&mut self, index: usize, submenu: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = &mut self.menu else {
            return;
        };
        let entry = if submenu {
            match menu.entries.get(menu.selected) {
                Some(Entry::Submenu { entries, .. }) => entries.borrow().get(index).cloned(),
                _ => None,
            }
        } else {
            menu.entries.get(index).cloned()
        };
        match entry {
            Some(Entry::Action {
                enabled: true,
                dismiss,
                handler,
                ..
            }) => {
                if dismiss {
                    self.dismiss(window, cx);
                }
                window.defer(cx, move |window, cx| handler(window, cx));
            }
            Some(Entry::Submenu { entries, .. }) => {
                menu.selected = index;
                menu.submenu_open = true;
                menu.submenu_selected = entries.borrow().iter().position(Entry::enabled);
                cx.notify();
            }
            _ => {} // Disabled entries are intrinsically non-dispatchable, including Enter.
        }
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_none() {
            return;
        }
        let key = event.keystroke.key.as_str();
        if !matches!(
            key,
            "escape" | "up" | "down" | "left" | "right" | "enter" | "tab"
        ) {
            return;
        }
        cx.stop_propagation();
        if matches!(key, "escape" | "tab") {
            self.dismiss(window, cx);
            return;
        }
        let menu = self.menu.as_mut().expect("open menu");
        if key == "left" {
            menu.submenu_open = false;
            menu.submenu_selected = None;
            cx.notify();
            return;
        }
        if key == "right" {
            if matches!(menu.entries.get(menu.selected), Some(Entry::Submenu { .. })) {
                let index = menu.selected;
                self.choose(index, false, window, cx);
            }
            return;
        }
        let in_submenu = menu.submenu_open && menu.submenu_selected.is_some();
        if key == "enter" {
            let index = if in_submenu {
                menu.submenu_selected.unwrap()
            } else {
                menu.selected
            };
            self.choose(index, in_submenu, window, cx);
            return;
        }
        let submenu_entries = match menu.entries.get(menu.selected) {
            Some(Entry::Submenu { entries, .. }) if in_submenu => Some(entries.borrow()),
            _ => None,
        };
        let entries = submenu_entries.as_deref().unwrap_or(&menu.entries);
        let current = if in_submenu {
            menu.submenu_selected.unwrap()
        } else {
            menu.selected
        };
        for step in 1..=entries.len() {
            let next = if key == "down" {
                (current + step) % entries.len()
            } else {
                (current + entries.len() - step % entries.len()) % entries.len()
            };
            if entries[next].enabled() {
                if in_submenu {
                    menu.submenu_selected = Some(next);
                    menu.submenu_scroll.scroll_to_item(next);
                } else {
                    menu.selected = next;
                    menu.submenu_open = false;
                    menu.root_scroll.scroll_to_item(next);
                }
                break;
            }
        }
        cx.notify();
    }
    fn render_entry(
        &self,
        entry: &Entry,
        index: usize,
        submenu: bool,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if matches!(entry, Entry::Separator) {
            return div()
                .h(px(1.0))
                .my(px(4.0))
                .mx(px(8.0))
                .bg(rgb(0x454545))
                .into_any_element();
        }
        let (label, icon, checked, arrow) = match entry {
            Entry::Action {
                label,
                icon,
                checked,
                ..
            } => (label.as_str(), *icon, *checked, false),
            Entry::Submenu { label, icon, .. } => (*label, *icon, false, true),
            Entry::Separator => unreachable!(),
        };
        div()
            .id((
                if submenu {
                    "track-submenu-item"
                } else {
                    "track-menu-item"
                },
                index,
            ))
            .h(px(40.0))
            .w_full()
            .px(px(8.0))
            .rounded(px(2.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .text_size(px(14.0))
            .text_color(rgb(if entry.enabled() { 0xffffff } else { 0x999999 }))
            .when(selected && entry.enabled(), |div| div.bg(rgb(0x3e3e3e)))
            .when(entry.enabled(), |div| div.cursor_pointer())
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered {
                    this.select(index, submenu, cx);
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.choose(index, submenu, window, cx);
            }))
            .child(div().size(px(16.0)).flex_shrink_0().child(if checked {
                super::liked::liked_check(16.0)
            } else {
                svg()
                    .path(icon)
                    .size(px(16.0))
                    .text_color(rgb(if entry.enabled() { 0xffffff } else { 0x999999 }))
                    .into_any_element()
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_ellipsis()
                    .child(label.to_string()),
            )
            .children(arrow.then(|| {
                svg()
                    .path("icons/chevron-right.svg")
                    .size(px(16.0))
                    .text_color(rgb(0xffffff))
            }))
            .into_any_element()
    }
    fn render_popup(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let menu = self.menu.as_ref().expect("open menu");
        let max_height = window.viewport_size().height - px(16.0);
        let root = div()
            .id("track-menu-popup")
            .occlude()
            .w(px(320.0))
            .max_h(max_height)
            .debug_selector(|| "track-menu-popup".into())
            .p(px(4.0))
            .rounded(px(4.0))
            .bg(rgb(0x282828))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("track-menu-rows")
                    .max_h(max_height - px(8.0))
                    .overflow_y_scroll()
                    .track_scroll(&menu.root_scroll)
                    .children(menu.entries.iter().enumerate().map(|(index, entry)| {
                        self.render_entry(entry, index, false, menu.selected == index, cx)
                    })),
            );
        // Submenus are siblings, not clipped descendants of the scrolling root.
        // Choose the side before anchoring, so a right-edge menu opens to its left.
        let root_height: f32 = menu
            .entries
            .iter()
            .map(|entry| {
                if matches!(entry, Entry::Separator) {
                    9.0
                } else {
                    40.0
                }
            })
            .sum::<f32>()
            + 8.0;
        let viewport = window.viewport_size();
        let x = menu
            .position
            .x
            .min((viewport.width - px(328.0)).max(px(8.0)))
            .max(px(8.0));
        let y = menu
            .position
            .y
            .min(
                (viewport.height - px(root_height.min(f32::from(max_height))) - px(8.0))
                    .max(px(8.0)),
            )
            .max(px(8.0));
        let mut surface = div().child(
            anchored()
                .position(point(x, y))
                .snap_to_window_with_margin(Edges::all(px(8.0)))
                .child(root),
        );
        if menu.submenu_open {
            if let Some(Entry::Submenu { entries, .. }) = menu.entries.get(menu.selected) {
                let entries = entries.borrow();
                let offset: f32 = menu.entries[..menu.selected]
                    .iter()
                    .map(|entry| {
                        if matches!(entry, Entry::Separator) {
                            9.0
                        } else {
                            40.0
                        }
                    })
                    .sum();
                let submenu_x = if x + px(644.0) <= viewport.width {
                    x + px(320.0)
                } else {
                    (x - px(320.0)).max(px(8.0))
                };
                let submenu = div()
                    .id("track-menu-submenu")
                    .occlude()
                    .w(px(320.0))
                    .p(px(4.0))
                    .rounded(px(4.0))
                    .bg(rgb(0x282828))
                    .shadow_lg()
                    .debug_selector(|| "track-menu-submenu".into())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id("track-submenu-rows")
                            .max_h(max_height - px(8.0))
                            .overflow_y_scroll()
                            .track_scroll(&menu.submenu_scroll)
                            .children(entries.iter().enumerate().map(|(index, entry)| {
                                self.render_entry(
                                    entry,
                                    index,
                                    true,
                                    menu.submenu_selected == Some(index),
                                    cx,
                                )
                            })),
                    );
                surface = surface.child(
                    anchored()
                        .position(point(
                            submenu_x,
                            y + px(offset) + menu.root_scroll.offset().y,
                        ))
                        .snap_to_window_with_margin(Edges::all(px(8.0)))
                        .child(submenu),
                );
            }
        }
        surface.into_any_element()
    }
}

enum Outcome {
    Done,
    Removed,
    Source(PlaylistSource),
    Artist(String, String),
    Credits(TrackCredits),
}
fn execute(session: Rc<MenuSession>, command: Command, window: &mut Window, cx: &mut App) {
    let Some(host) = session.host.upgrade() else {
        return;
    };
    if matches!(command, Command::Album) {
        super::track_title::open_track_album(&session.target.uri, cx);
        return;
    }
    if let Command::Timer(choice) = command {
        host.update(cx, |host, cx| {
            host.message = Some(match session.client.set_sleep_timer(choice) {
                Ok(()) => (
                    match choice {
                        SleepTimerChoice::Off => "Sleep timer turned off".to_string(),
                        SleepTimerChoice::EndOfTrack => "Sleep timer: end of track".to_string(),
                        SleepTimerChoice::Minutes(minutes) => {
                            format!("Sleep timer: {minutes} minutes")
                        }
                    },
                    false,
                ),
                Err(error) => (error.to_string(), true),
            });
            cx.notify();
        });
        return;
    }
    let liked = if matches!(command, Command::Like) {
        match session.client.liked_status(&session.target.uri) {
            LikedStatus::Liked => Some(false),
            LikedStatus::NotLiked => Some(true),
            LikedStatus::Unknown => {
                session.client.ensure_liked_tracks_loaded();
                host.update(cx, |host, cx| {
                    host.message = Some(("Liked Songs membership is still loading. Reopen the menu to retry; no change was made.".into(), true)); cx.notify();
                });
                return;
            }
        }
    } else {
        None
    };
    let key = format!(
        "{}:{}",
        match &command {
            Command::Like => "like",
            Command::Queue => "queue",
            Command::Remove => "remove",
            Command::Add(uri) => uri,
            Command::Radio => "radio",
            Command::Artist => "artist",
            Command::Credits => "credits",
            Command::Album | Command::Timer(_) => unreachable!("local command handled above"),
        },
        session.target.uri
    );
    let credits = matches!(command, Command::Credits);
    let generation = host.update(cx, |host, cx| {
        if !host.pending.insert(key.clone()) {
            return None;
        }
        host.message = Some((format!("Working on {}…", session.target.title), false));
        if credits {
            host.credits = None;
            host.credits_loading = true;
            host.credits_generation += 1;
            host.credits_previous_focus = window.focused(cx);
            window.focus(&host.focus);
        }
        cx.notify();
        Some(host.credits_generation)
    });
    let Some(generation) = generation else {
        return;
    };
    let client = session.client.clone();
    let target = session.target.clone();
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let result = (|| {
            match command {
                Command::Like => client
                    .set_liked(&target.uri, liked.expect("resolved membership"))
                    .map(|_| Outcome::Done),
                Command::Queue => client.add_to_queue(&target.uri).map(|_| Outcome::Done),
                Command::Add(uri) => client
                    .add_track_to_playlist(&uri, &target.uri)
                    .map(|_| Outcome::Done),
                Command::Remove => match target.occurrence.as_ref() {
                    Some(occurrence) => client
                        .remove_playlist_occurrence(occurrence)
                        .map(|_| Outcome::Removed),
                    None => {
                        return Err("This track has no editable playlist occurrence".to_string());
                    }
                },
                Command::Radio => client.track_radio(&target.uri).map(Outcome::Source),
                Command::Credits => client.track_credits(&target.uri).map(Outcome::Credits),
                Command::Artist => {
                    let info = client
                        .track_menu_info(&target.uri)
                        .map_err(|error| error.to_string())?;
                    let Some(artist) = info
                        .artists
                        .into_iter()
                        .find(|artist| artist.uri.starts_with("spotify:artist:"))
                    else {
                        return Err("No artist identity was returned for this track".into());
                    };
                    return Ok(Outcome::Artist(artist.uri, artist.name));
                }
                Command::Album | Command::Timer(_) => unreachable!("local command handled above"),
            }
            .map_err(|error| error.to_string())
        })();
        let _ = tx.send(result);
    });
    window
        .spawn(cx, async move |cx| {
            let result = rx
                .await
                .unwrap_or_else(|_| Err("Track action worker stopped".into()));
            let _ = session.host.update_in(cx, |host, window, cx| {
                host.pending.remove(&key);
                let show_credits =
                    credits && generation == host.credits_generation && host.credits_loading;
                if show_credits {
                    host.credits_loading = false;
                }
                match result {
                    Err(error) => {
                        if show_credits {
                            if let Some(focus) = host.credits_previous_focus.take() {
                                window.focus(&focus);
                            } else {
                                window.blur();
                            }
                        }
                        host.message = Some((error, true));
                    }
                    Ok(Outcome::Credits(value)) => {
                        if show_credits {
                            host.credits = Some(value);
                        }
                        host.message = None;
                    }
                    Ok(Outcome::Source(source)) => {
                        host.message = open_source(source, session.client.clone(), cx)
                            .err()
                            .map(|error| (error, true))
                    }
                    Ok(Outcome::Artist(uri, name)) => {
                        if cx.try_global::<StackRouterHandle>().is_some() {
                            open_artist_profile(&uri, &name, None, cx);
                            host.message = None;
                        } else {
                            host.message = Some(("Navigation is not available".into(), true));
                        }
                    }
                    Ok(Outcome::Removed) => {
                        host.message = Some(("Track removed from playlist".into(), false));
                        if let Some(removed) = &session.removed {
                            removed(cx);
                        }
                    }
                    Ok(Outcome::Done) => {
                        host.message = Some(("Track action completed".into(), false))
                    }
                }
                cx.notify();
            });
        })
        .detach();
}
fn open_source(source: PlaylistSource, client: NativeSpotify, cx: &mut App) -> Result<(), String> {
    let Some(router) = cx.try_global::<StackRouterHandle>().cloned() else {
        return Err("Navigation is not available".into());
    };
    let uri = match &source {
        PlaylistSource::Playlist { uri, .. } | PlaylistSource::Context { uri, .. } => uri.clone(),
        PlaylistSource::LikedSongs => "spotify:collection:tracks".into(),
        PlaylistSource::TrackAlbum { track_uri } => format!("album-for:{track_uri}"),
    };
    if router.current_source(cx) != Some(uri.as_str()) {
        let screen = cx.new(|cx| PlaylistScreen::new(source, client, cx));
        router.push_for_source(screen.into(), uri, cx);
    }
    Ok(())
}
impl TrackMenuHost {
    fn close_credits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.credits = None;
        self.credits_loading = false;
        self.credits_generation += 1;
        if let Some(focus) = self.credits_previous_focus.take() {
            window.focus(&focus);
        } else {
            window.blur();
        }
        cx.notify();
    }
}
impl Render for TrackMenuHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal = self.credits.is_some() || self.credits_loading;
        let mut root = div().id("track-menu-host").absolute().inset_0();
        if let Some((message, error)) = &self.message {
            root = root.child(
                div()
                    .id("track-action-message")
                    .absolute()
                    .bottom(px(90.0))
                    .left(px(24.0))
                    .max_w(px(560.0))
                    .p(px(16.0))
                    .rounded(px(6.0))
                    .bg(rgb(if *error { 0x542323 } else { 0x282828 }))
                    .text_color(rgb(0xffffff))
                    .text_size(px(14.0))
                    .flex()
                    .gap(px(16.0))
                    .child(message.clone())
                    .child(
                        div()
                            .id("dismiss-track-message")
                            .cursor_pointer()
                            .child("×")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.message = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        if self.menu.is_some() {
            root = root
                .occlude()
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.dismiss(window, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.dismiss(window, cx);
                    }),
                )
                .child(deferred(self.render_popup(window, cx)).with_priority(10));
        }
        if modal {
            let mut content = div().flex().flex_col().gap(px(14.0));
            if let Some(credits) = &self.credits {
                content = content.child(div().text_2xl().child(credits.title.clone()));
                if credits.groups.is_empty() {
                    content = content.child("No credits were returned for this track.");
                }
                for group in &credits.groups {
                    content = content.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(div().text_color(rgb(0xb3b3b3)).child(group.role.clone()))
                            .children(group.names.iter().cloned().map(|name| div().child(name))),
                    );
                }
                if !credits.source_names.is_empty() {
                    content = content.child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xb3b3b3))
                            .child(format!("Source: {}", credits.source_names.join(", "))),
                    );
                }
            } else {
                content = content.child("Loading song credits…");
            }
            root = root
                .occlude()
                .bg(rgba(0x00000099))
                .flex()
                .items_center()
                .justify_center()
                .track_focus(&self.focus)
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.close_credits(window, cx);
                        cx.stop_propagation();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_credits(window, cx)),
                )
                .child(
                    div()
                        .id("song-credits-dialog")
                        .w(px(480.0))
                        .max_h(px(560.0))
                        .overflow_y_scroll()
                        .p(px(24.0))
                        .rounded(px(8.0))
                        .bg(rgb(0x282828))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .mb(px(20.0))
                                .child("Song credits")
                                .child(
                                    div()
                                        .id("close-song-credits")
                                        .cursor_pointer()
                                        .child("×")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.close_credits(window, cx)
                                        })),
                                ),
                        )
                        .child(content),
                );
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, size};
    use std::cell::Cell;

    struct Fixture {
        host: Entity<TrackMenuHost>,
        tracks: Vec<(usize, String)>,
        played: Rc<Cell<usize>>,
        actions: Rc<RefCell<Vec<(usize, String, String)>>>,
        destinations: Entries,
        clipped: bool,
        offset: f32,
    }
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let rows = self
                .tracks
                .iter()
                .map(|(occurrence, uri)| {
                    let occurrence = *occurrence;
                    let uri = uri.clone();
                    let played = self.played.clone();
                    let host = self.host.clone();
                    let actions = self.actions.clone();
                    let destinations = self.destinations.clone();
                    let scope: SharedString = format!("fixture-occurrence-{occurrence}").into();
                    target_surface(
                        div()
                            .id(("fixture-track", occurrence))
                            .w(px(240.0))
                            .h(px(56.0))
                            .on_click(move |_, _, _| played.set(played.get() + 1))
                            .child(format!("Track {occurrence}")),
                        scope.clone(),
                        move |position, window, cx| {
                            let actions = actions.clone();
                            let uri = uri.clone();
                            let share_uri = uri.clone();
                            let dispatch: Dispatch = Rc::new(move |command, _, _| {
                                let command = match command {
                                    Command::Remove => "remove".to_string(),
                                    Command::Queue => "queue".to_string(),
                                    Command::Add(uri) => format!("add:{uri}"),
                                    _ => "other".to_string(),
                                };
                                actions
                                    .borrow_mut()
                                    .push((occurrence, uri.clone(), command));
                            });
                            *destinations.borrow_mut() = vec![action(
                                "Editable playlist",
                                "icons/list.svg",
                                Command::Add("spotify:playlist:destination".into()),
                                &dispatch,
                            )];
                            let entries = menu_entries(
                                true,
                                LikedStatus::Liked,
                                SleepTimerChoice::Off,
                                dispatch,
                                destinations.clone(),
                                &share_uri,
                            );
                            host.update(cx, |host, cx| {
                                host.open(entries, scope.clone(), position, window, cx)
                            });
                        },
                    )
                })
                .collect::<Vec<_>>();
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .id("fixture-viewport")
                        .ml(px(self.offset))
                        .w(px(240.0))
                        .when(self.clipped, |div| div.h(px(56.0)).overflow_hidden())
                        .children(rows),
                )
                .child(self.host.clone())
        }
    }

    fn fixture(cx: &mut TestAppContext) -> (Entity<Fixture>, &mut gpui::VisualTestContext) {
        let (view, cx) = cx.add_window_view(|_, cx| Fixture {
            host: init(cx),
            tracks: vec![
                (0, "spotify:track:duplicate".into()),
                (1, "spotify:track:duplicate".into()),
            ],
            played: Rc::new(Cell::new(0)),
            actions: Rc::new(RefCell::new(Vec::new())),
            destinations: Rc::new(RefCell::new(Vec::new())),
            clipped: false,
            offset: 0.0,
        });
        cx.simulate_resize(size(px(900.0), px(700.0)));
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(900.0), px(700.0)),
            |_, _| view.clone(),
        );
        (view, cx)
    }

    #[gpui::test]
    fn right_click_keeps_playback_and_targets_clicked_occurrence_after_reorder(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = fixture(cx);
        cx.simulate_mouse_down(
            point(px(10.0), px(70.0)),
            MouseButton::Right,
            Default::default(),
        );
        cx.simulate_mouse_up(
            point(px(10.0), px(70.0)),
            MouseButton::Right,
            Default::default(),
        );
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.tracks.reverse();
                cx.notify();
            })
        });
        cx.simulate_keystrokes("down enter"); // Remove the captured second occurrence.
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.played.get(), 0);
            assert_eq!(
                *view.actions.borrow(),
                vec![(1, "spotify:track:duplicate".into(), "remove".into())]
            );
            assert!(view.host.read(cx).menu.is_none());
        });
    }

    #[gpui::test]
    fn escape_and_outside_click_dismiss_without_activating_track(cx: &mut TestAppContext) {
        let (view, cx) = fixture(cx);
        cx.simulate_mouse_down(
            point(px(10.0), px(10.0)),
            MouseButton::Right,
            Default::default(),
        );
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| assert!(view.read(cx).host.read(cx).menu.is_none()));
        cx.simulate_mouse_down(
            point(px(10.0), px(10.0)),
            MouseButton::Right,
            Default::default(),
        );
        cx.simulate_click(point(px(700.0), px(600.0)), Default::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.host.read(cx).menu.is_none());
            assert_eq!(view.played.get(), 0);
            assert!(view.actions.borrow().is_empty());
        });
        cx.simulate_click(point(px(10.0), px(10.0)), Default::default());
        cx.update(|_, cx| assert_eq!(view.read(cx).played.get(), 1));
    }

    #[gpui::test]
    fn playlist_submenu_accepts_keyboard_selection_and_dismisses(cx: &mut TestAppContext) {
        let (view, cx) = fixture(cx);
        cx.simulate_mouse_down(
            point(px(10.0), px(10.0)),
            MouseButton::Right,
            Default::default(),
        );
        cx.simulate_keystrokes("right enter");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(
                *view.actions.borrow(),
                vec![(
                    0,
                    "spotify:track:duplicate".into(),
                    "add:spotify:playlist:destination".into()
                )]
            );
            assert!(view.host.read(cx).menu.is_none());
            assert_eq!(view.played.get(), 0);
        });
    }

    #[gpui::test]
    fn disabled_hover_enter_never_dispatches_and_keyboard_skips_it(cx: &mut TestAppContext) {
        let (view, cx) = fixture(cx);
        cx.simulate_mouse_down(
            point(px(10.0), px(10.0)),
            MouseButton::Right,
            Default::default(),
        );
        cx.simulate_mouse_move(point(px(100.0), px(194.0)), None, Default::default()); // Taste profile.
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.host.read(cx).menu.is_some());
            assert!(view.actions.borrow().is_empty());
        });
        cx.simulate_keystrokes("down right");
        cx.update(|_, cx| {
            let view = view.read(cx);
            let host = view.host.read(cx);
            let menu = host.menu.as_ref().unwrap();
            assert_eq!(menu.selected, 6, "Down skips disabled Jam to Sleep timer");
            assert!(menu.submenu_open);
            assert_eq!(menu.submenu_selected, Some(0));
        });
    }

    #[gpui::test]
    fn clipped_virtual_target_does_not_open_menu(cx: &mut TestAppContext) {
        let (view, cx) = fixture(cx);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.clipped = true;
                cx.notify();
            })
        });
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(900.0), px(700.0)),
            |_, _| view.clone(),
        );
        cx.simulate_mouse_down(
            point(px(10.0), px(70.0)),
            MouseButton::Right,
            Default::default(),
        );
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.host.read(cx).menu.is_none());
            assert!(view.actions.borrow().is_empty());
        });
    }

    #[gpui::test]
    fn edge_submenu_flips_left_and_mouse_choice_dismisses(cx: &mut TestAppContext) {
        let (view, cx) = fixture(cx);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.offset = 620.0;
                cx.notify();
            })
        });
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(900.0), px(700.0)),
            |_, _| view.clone(),
        );
        cx.simulate_mouse_down(
            point(px(840.0), px(10.0)),
            MouseButton::Right,
            Default::default(),
        );
        cx.simulate_keystrokes("right");
        let root = cx
            .debug_bounds("track-menu-popup")
            .expect("production popup drawn");
        let submenu = cx
            .debug_bounds("track-menu-submenu")
            .expect("production submenu drawn");
        assert!(root.left() >= px(8.0) && root.right() <= px(892.0));
        assert!(submenu.left() >= px(8.0) && submenu.right() <= root.left());
        cx.simulate_click(
            submenu.origin + point(px(60.0), px(24.0)),
            Default::default(),
        );
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(
                *view.actions.borrow(),
                vec![(
                    0,
                    "spotify:track:duplicate".into(),
                    "add:spotify:playlist:destination".into()
                )]
            );
            assert!(view.host.read(cx).menu.is_none());
            assert_eq!(view.played.get(), 0);
        });
    }
}
