use gpui::{ClipboardItem, WeakEntity};
use gpui_component::{
    Icon,
    menu::{PopupMenu, PopupMenuItem},
    popover::{Popover, PopoverState},
};

use super::*;
use crate::view::components::smooth_scroll::{ScrollAxis, SmoothScrollElement};

impl LibrarySidebar {
    pub(super) fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let collapsed = self.size == LibrarySize::Collapsed;
        let expanded = self.size == LibrarySize::Expanded;
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(64.0))
            .px(px(16.0))
            .gap(px(8.0))
            .flex_shrink_0()
            .child(
                div()
                    .id("library-heading-container")
                    .flex_1()
                    .min_w(px(0.0))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.heading_hovered = *hovered;
                        cx.notify();
                    }))
                    .child(
                        Button::new("library-heading")
                            .ghost()
                            .px(px(4.0))
                            .h(px(40.0))
                            .justify_start()
                            .when(collapsed, |b| {
                                b.w(px(40.0)).child(icon("icons/library.svg", 24.0, MUTED))
                            })
                            .when(!collapsed, |b| {
                                b.when(self.heading_hovered, |b| {
                                    b.icon(
                                        Icon::default()
                                            .path("icons/library-collapse.svg")
                                            .text_color(rgb(MUTED)),
                                    )
                                })
                                .child(
                                    div()
                                        .text_size(px(16.0))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(rgb(0xffffff))
                                        .child("Your Library"),
                                )
                            })
                            .tooltip(if collapsed {
                                "Open Your Library"
                            } else {
                                "Collapse Your Library"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.change_size(
                                    if collapsed {
                                        this.last_open_size
                                    } else {
                                        LibrarySize::Collapsed
                                    },
                                    cx,
                                );
                            })),
                    ),
            )
            .when(!collapsed, |d| {
                d.child(self.render_add_button(cx)).child(
                    Button::new("library-expand")
                        .ghost()
                        .size(px(36.0))
                        .rounded_full()
                        .child(icon(
                            if expanded {
                                "icons/collapse.svg"
                            } else {
                                "icons/expand.svg"
                            },
                            16.0,
                            MUTED,
                        ))
                        .tooltip(if expanded {
                            "Minimize Your Library"
                        } else {
                            "Expand Your Library"
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.change_size(
                                if expanded {
                                    LibrarySize::Normal
                                } else {
                                    LibrarySize::Expanded
                                },
                                cx,
                            )
                        })),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_add_button(&self, _cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.size == LibrarySize::Expanded;
        Popover::new("library-create-menu")
            .appearance(false)
            .anchor(Corner::TopRight)
            .trigger(
                Button::new("library-add")
                    .ghost()
                    .h(px(36.0))
                    .rounded_full()
                    .bg(rgb(0x242424))
                    .when(!expanded, |b| b.w(px(36.0)))
                    .when(expanded, |b| {
                        b.px(px(16.0))
                            .icon(
                                Icon::default()
                                    .path("icons/plus.svg")
                                    .text_color(rgb(MUTED)),
                            )
                            .label("Create")
                            .font_weight(gpui::FontWeight::BOLD)
                    })
                    .when(!expanded, |b| b.child(icon("icons/plus.svg", 16.0, MUTED)))
                    .tooltip("Create a playlist, folder, or Jam"),
            )
            .content(|_, _, _| {
                div()
                    .w(px(292.0))
                    .p(px(4.0))
                    .rounded(px(4.0))
                    .bg(rgb(0x282828))
                    .shadow_lg()
                    .font_family("Spotify Mix UI")
                    .children(
                        [
                            (
                                "Playlist",
                                "Build a playlist with songs you love",
                                "icons/list.svg",
                            ),
                            ("Folder", "Organize your playlists", "icons/folder.svg"),
                            (
                                "Jam",
                                "Listen together with friends",
                                "icons/friends-outlined.svg",
                            ),
                        ]
                        .into_iter()
                        .map(|(title, subtitle, path)| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .p(px(12.0))
                                .rounded(px(4.0))
                                .opacity(0.5)
                                .child(icon(path, 24.0, MUTED))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .text_color(rgb(0xffffff))
                                                .child(title),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(rgb(MUTED))
                                                .child(subtitle),
                                        ),
                                )
                        }),
                    )
                    .child(
                        div()
                            .p(px(12.0))
                            .text_size(px(12.0))
                            .text_color(rgb(MUTED))
                            .child("Native creation and Jam services are not connected yet."),
                    )
            })
            .into_any_element()
    }

    pub(super) fn render_chips(&self, cx: &mut Context<Self>) -> AnyElement {
        let (left, right) = self.chip_edges;
        let mut chips = Vec::new();
        if self.filter.is_some() {
            chips.push(
                Button::new("library-clear-filter")
                    .ghost()
                    .size(px(32.0))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(rgb(0x303030))
                    .child(icon("icons/close.svg", 20.0, MUTED))
                    .tooltip("Clear filters")
                    .on_click(cx.listener(|this, _, _, cx| this.select_filter(None, cx)))
                    .into_any_element(),
            );
        }
        if self.filter == Some(Filter::Playlists) && self.ownership != Ownership::All {
            chips.push(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(rgb(0xd6d6d6))
                    .child(
                        Button::new("library-primary-filter")
                            .ghost()
                            .h(px(32.0))
                            .px(px(12.0))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .text_color(rgb(0x000000))
                            .text_size(px(14.0))
                            .label("Playlists")
                            .on_click(cx.listener(|this, _, _, cx| this.select_filter(None, cx))),
                    )
                    .child(
                        Button::new("library-owner-filter")
                            .ghost()
                            .h(px(32.0))
                            .px(px(12.0))
                            .rounded_full()
                            .text_color(rgb(0x000000))
                            .text_size(px(14.0))
                            .label(self.ownership.label())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.ownership = Ownership::All;
                                this.rebuild();
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
            );
        } else if let Some(filter) = self.filter {
            chips.push(self.category_chip(filter, true, cx));
            if filter == Filter::Playlists {
                for ownership in [Ownership::ByYou, Ownership::BySpotify] {
                    chips.push(
                        Button::new(ownership.label())
                            .ghost()
                            .h(px(32.0))
                            .px(px(12.0))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(rgb(0x303030))
                            .text_size(px(14.0))
                            .text_color(rgb(0xffffff))
                            .label(ownership.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ownership = ownership;
                                this.rebuild();
                                cx.notify();
                            }))
                            .into_any_element(),
                    );
                }
            }
        } else {
            chips.extend(
                Filter::ALL
                    .into_iter()
                    .map(|filter| self.category_chip(filter, false, cx)),
            );
        }
        div()
            .relative()
            .h(px(44.0))
            .mx(px(16.0))
            .flex_shrink_0()
            .overflow_hidden()
            .flex()
            .items_center()
            .child(
                div()
                    .id("library-chips")
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .w_full()
                    .smooth_scroll(
                        self.chips.clone(),
                        ScrollAxis::Horizontal,
                        self.chip_scroll.clone(),
                    )
                    .children(chips),
            )
            .when(left, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(6.0))
                        .w(px(48.0))
                        .h(px(32.0))
                        .bg(linear_gradient(
                            90.0,
                            linear_color_stop(rgb(BG), 0.0),
                            linear_color_stop(rgba(0x19191900), 1.0),
                        )),
                )
                .child(
                    Button::new("library-chips-left")
                        .ghost()
                        .absolute()
                        .left_0()
                        .size(px(32.0))
                        .rounded_full()
                        .bg(rgb(0x242424))
                        .child(icon("icons/back.svg", 16.0, MUTED))
                        .tooltip("Scroll left")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let offset = this.chips.offset();
                            this.chip_scroll.cancel();
                            this.chips
                                .set_offset(point((offset.x + px(160.0)).min(px(0.0)), px(0.0)));
                            cx.notify();
                        })),
                )
            })
            .when(right, |d| {
                d.child(
                    div()
                        .absolute()
                        .right_0()
                        .top(px(6.0))
                        .w(px(48.0))
                        .h(px(32.0))
                        .bg(linear_gradient(
                            90.0,
                            linear_color_stop(rgba(0x19191900), 0.0),
                            linear_color_stop(rgb(BG), 0.75),
                        )),
                )
                .child(
                    Button::new("library-chips-right")
                        .ghost()
                        .absolute()
                        .right_0()
                        .size(px(32.0))
                        .rounded_full()
                        .bg(rgb(0x242424))
                        .child(icon("icons/forward.svg", 16.0, MUTED))
                        .tooltip("Scroll right")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let offset = this.chips.offset();
                            this.chip_scroll.cancel();
                            this.chips.set_offset(point(
                                (offset.x - px(160.0)).max(-this.chips.max_offset().width),
                                px(0.0),
                            ));
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    fn category_chip(&self, filter: Filter, selected: bool, cx: &mut Context<Self>) -> AnyElement {
        Button::new(filter.label())
            .ghost()
            .h(px(32.0))
            .px(px(12.0))
            .flex_shrink_0()
            .rounded_full()
            .bg(rgb(if selected { 0xffffff } else { 0x303030 }))
            .text_size(px(14.0))
            .text_color(rgb(if selected { 0x000000 } else { 0xffffff }))
            .label(filter.label())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_filter(if selected { None } else { Some(filter) }, cx)
            }))
            .into_any_element()
    }

    pub(super) fn render_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.size == LibrarySize::Expanded;
        let search_open = self.search_open || expanded;
        let weak = cx.entity().downgrade();
        div()
            .h(px(48.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .flex_shrink_0()
            .when(!search_open, |d| {
                d.child(
                    Button::new("library-search")
                        .ghost()
                        .size(px(32.0))
                        .rounded_full()
                        .child(icon("icons/search.svg", 16.0, MUTED))
                        .tooltip("Search in Your Library")
                        .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx))),
                )
            })
            .when(search_open, |d| {
                d.child(
                    div()
                        .h(px(32.0))
                        .flex_1()
                        .min_w(px(0.0))
                        .max_w(px(240.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .rounded(px(4.0))
                        .bg(rgb(0x333333))
                        .child(icon("icons/search.svg", 16.0, MUTED))
                        .child(
                            Input::new(&self.search)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .cleanable(true)
                                .flex_1()
                                .min_w(px(0.0))
                                .text_size(px(14.0))
                                .text_color(rgb(0xffffff)),
                        ),
                )
            })
            .child(
                Popover::new("library-sort-popup")
                    .appearance(false)
                    .anchor(Corner::TopRight)
                    .trigger(
                        Button::new("library-sort")
                            .ghost()
                            .px(px(4.0))
                            .h(px(32.0))
                            .text_color(rgb(MUTED))
                            .text_size(px(14.0))
                            .when(!search_open || expanded, |b| b.label(self.sort.label()))
                            .child(icon(self.view.icon(), 16.0, MUTED))
                            .tooltip("Sort by and view as"),
                    )
                    .content(move |_, window, cx| {
                        let popup = cx.entity().downgrade();
                        weak.update(cx, |this, cx| this.render_sort_menu(popup, window, cx))
                            .unwrap_or_else(|_| div().into_any_element())
                    }),
            )
            .into_any_element()
    }

    fn render_sort_menu(
        &self,
        popup: WeakEntity<PopoverState>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut menu = div()
            .w(px(208.0))
            .p(px(4.0))
            .rounded(px(4.0))
            .bg(rgb(0x282828))
            .shadow_lg()
            .font_family("Spotify Mix UI")
            .child(
                div()
                    .px(px(12.0))
                    .py(px(12.0))
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(MUTED))
                    .child("Sort by"),
            );
        for choice in LibrarySort::ALL {
            // Native unfiltered library normally omits Custom order. Keep it available here
            // because it is a supported fallback if native recents cannot be loaded.
            if choice == LibrarySort::Custom
                && self.filter != Some(Filter::Playlists)
                && self.has_recents()
                && self.sort != LibrarySort::Custom
            {
                continue;
            }
            let selected = self.sort == choice;
            let available = choice != LibrarySort::Recents || self.has_recents();
            let popup = popup.clone();
            menu = menu.child(
                div()
                    .id(choice.label())
                    .h(px(40.0))
                    .px(px(12.0))
                    .rounded(px(2.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(14.0))
                    .text_color(rgb(if selected {
                        GREEN
                    } else if available {
                        0xffffff
                    } else {
                        MUTED
                    }))
                    .when(available, |d| {
                        d.cursor_pointer().hover(|s| s.bg(rgb(0x3a3a3a)))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !available {
                            return;
                        }
                        this.reverse_sort =
                            selected && !this.reverse_sort && choice != LibrarySort::Custom;
                        this.sort = choice;
                        this.sort_explicit = true;
                        this.rebuild();
                        cx.notify();
                        let _ = popup.update(cx, |popup, cx| popup.dismiss(window, cx));
                    }))
                    .child(choice.label())
                    .when(selected, |d| d.child(icon("icons/check.svg", 20.0, GREEN))),
            );
        }
        menu.child(div().h(px(1.0)).my(px(8.0)).bg(rgb(0x3e3e3e)))
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(MUTED))
                    .child("View as"),
            )
            .child(
                div()
                    .flex()
                    .p(px(4.0))
                    .rounded(px(8.0))
                    .bg(rgb(0x242424))
                    .children(LibraryView::ALL.into_iter().map(|choice| {
                        let selected = self.view == choice;
                        Button::new(choice.label())
                            .ghost()
                            .flex_1()
                            .h(px(32.0))
                            .rounded(px(4.0))
                            .when(selected, |b| b.bg(rgb(0x414141)))
                            .child(icon(
                                choice.icon(),
                                20.0,
                                if selected { GREEN } else { MUTED },
                            ))
                            .tooltip(choice.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.view = choice;
                                this.rebuild();
                                cx.notify();
                            }))
                    })),
            )
            .into_any_element()
    }

    pub(super) fn playlist_menu(
        menu: PopupMenu,
        entry: LibraryEntry,
        weak: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let share_url = entry.share_url();
        let noun = match entry.kind {
            LibraryKind::Album => "album",
            LibraryKind::Artist => "artist",
            LibraryKind::Show => "podcast",
            LibraryKind::Folder => "folder",
            _ => "playlist",
        };
        let refresh = weak.clone();
        let pin = weak.clone();
        let pin_entry = entry.clone();
        let pin_busy = weak.upgrade().is_none_or(|entity| entity.read(cx).pin_busy);
        menu.item(
            PopupMenuItem::new("Add to queue")
                .icon(Icon::default().path("icons/queue-add.svg"))
                .disabled(true),
        )
        .item(
            PopupMenuItem::new("Start a Jam")
                .icon(Icon::default().path("icons/friends-outlined.svg"))
                .disabled(true),
        )
        .separator()
        .item(
            PopupMenuItem::new("Add to profile")
                .icon(Icon::default().path("icons/library.svg"))
                .disabled(true),
        )
        .separator()
        .item(
            PopupMenuItem::new("Report")
                .icon(Icon::default().path("icons/report.svg"))
                .disabled(true),
        )
        .item(
            PopupMenuItem::new("Add to Your Library")
                .icon(Icon::default().path("icons/plus.svg"))
                .disabled(true),
        )
        .item(
            PopupMenuItem::new("Download")
                .icon(Icon::default().path("icons/download.svg"))
                .disabled(true),
        )
        .separator()
        .item(
            PopupMenuItem::new("Create playlist")
                .icon(Icon::default().path("icons/queue-add.svg"))
                .disabled(true),
        )
        .item(
            PopupMenuItem::new("Create folder")
                .icon(Icon::default().path("icons/plus.svg"))
                .disabled(true),
        )
        .item(PopupMenuItem::new("Exclude from your taste profile").disabled(true))
        .item(
            PopupMenuItem::new("Move to folder")
                .icon(Icon::default().path("icons/folder.svg"))
                .disabled(true),
        )
        .item(
            PopupMenuItem::new("Add to other playlist")
                .icon(Icon::default().path("icons/plus.svg"))
                .disabled(true),
        )
        .item(
            PopupMenuItem::new(format!(
                "{} {noun}",
                if entry.pinned == Some(true) {
                    "Unpin"
                } else {
                    "Pin"
                }
            ))
            .icon(Icon::default().path("icons/pin.svg").text_color(rgb(
                if entry.pinned == Some(true) {
                    GREEN
                } else {
                    MUTED
                },
            )))
            .disabled(
                entry.pinned.is_none()
                    || pin_busy
                    || (entry.pinned != Some(true) && entry.parent.is_some()),
            )
            .on_click(move |_, _, cx| {
                let _ = pin.update(cx, |this, cx| this.toggle_pin(&pin_entry, cx));
            }),
        )
        .submenu_with_icon(
            Some(Icon::default().path("icons/share.svg")),
            "Share",
            window,
            cx,
            move |menu, _, _| {
                let link = share_url.clone();
                menu.item(PopupMenuItem::new(format!("Copy link to {noun}")).on_click(
                    move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                    },
                ))
            },
        )
        .separator()
        .item(
            PopupMenuItem::new("Refresh library").on_click(move |_, _, cx| {
                let _ = refresh.update(cx, |this, cx| this.refresh(cx));
            }),
        )
        .label("Disabled actions need additional native services")
    }
}
