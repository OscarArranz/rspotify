use gpui::{
    App, AppContext, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, div, px, rgb,
};
use librespot::core::SpotifyId;

use crate::lib::spotify::PlaylistSource;
use crate::router::StackRouterHandle;
use crate::state::AppState;
use crate::view::screens::playlist::PlaylistScreen;

fn canonical_uri(uri: &str, prefix: &str) -> bool {
    uri.strip_prefix(prefix)
        .is_some_and(|id| id.len() == 22 && SpotifyId::from_base62(id).is_ok())
}

pub(crate) fn player_track_title(title: &str, track_uri: &str) -> impl IntoElement {
    let text = div().min_w(px(0.0)).truncate().child(title.to_string());
    if !canonical_uri(track_uri, "spotify:track:") {
        return div().child(text).into_any_element();
    }

    let track_uri = track_uri.to_string();
    // The flex child keeps the hit target at the text's intrinsic width, shrinking
    // only when needed for ellipsis. Reserve the underline border to avoid hover-time shifts.
    div()
        .flex()
        .min_w(px(0.0))
        .child(
            text.id("track-title")
                .cursor_pointer()
                .border_b_1()
                .border_color(gpui::transparent_black())
                .hover(|style| style.border_color(rgb(0xffffff)))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    open_track_album(&track_uri, cx);
                }),
        )
        .into_any_element()
}

pub(crate) fn album_title(
    title: &str,
    album_uri: Option<&str>,
    track_uri: &str,
) -> impl IntoElement {
    let album_uri = album_uri
        .filter(|uri| canonical_uri(uri, "spotify:album:"))
        .map(str::to_string);
    let track_uri = (album_uri.is_none() && canonical_uri(track_uri, "spotify:track:"))
        .then(|| track_uri.to_string());
    let text = div().min_w(px(0.0)).truncate().child(title.to_string());
    if album_uri.is_none() && track_uri.is_none() {
        return div().child(text).into_any_element();
    }

    div()
        .flex()
        .min_w(px(0.0))
        .child(
            text.id("album-title")
                .cursor_pointer()
                .border_b_1()
                .border_color(gpui::transparent_black())
                .hover(|style| style.border_color(rgb(0xffffff)).text_color(rgb(0xffffff)))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if let Some(uri) = album_uri.as_deref() {
                        open_album(uri, cx);
                    } else if let Some(uri) = track_uri.as_deref() {
                        open_track_album(uri, cx);
                    }
                }),
        )
        .into_any_element()
}

/// Open a song's containing album, single or EP using native metadata on the destination worker.
pub(crate) fn open_track_album(track_uri: &str, cx: &mut App) {
    if !canonical_uri(track_uri, "spotify:track:") {
        return;
    }
    open_source(
        PlaylistSource::TrackAlbum {
            track_uri: track_uri.to_string(),
        },
        format!("album-for:{track_uri}"),
        cx,
    );
}

/// Navigate to an album without starting or interrupting playback.
pub(crate) fn open_album(uri: &str, cx: &mut App) {
    if !canonical_uri(uri, "spotify:album:") {
        return;
    }
    open_source(
        PlaylistSource::Context {
            uri: uri.to_string(),
            name: "Album".into(),
            owner: String::new(),
            image_url: None,
            kind: "Album",
        },
        uri.to_string(),
        cx,
    );
}

fn open_source(source: PlaylistSource, key: String, cx: &mut App) {
    let Some(router) = cx.try_global::<StackRouterHandle>().cloned() else {
        return;
    };
    if router.current_source(cx) == Some(key.as_str()) {
        return;
    }
    let Some(client) = cx
        .try_global::<AppState>()
        .and_then(AppState::spotify_client)
    else {
        return;
    };
    let screen = cx.new(|cx| PlaylistScreen::new(source, client, cx));
    router.push_for_source(screen.into(), key, cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::components::smooth_scroll::{SmoothScrollState, list_scroll_overlay};
    use gpui::{
        Context, ListAlignment, ListState, Render, TestAppContext, Window, list, point, size,
    };
    use std::{cell::Cell, rc::Rc};

    #[derive(Clone, Copy)]
    enum TitleKind {
        Player,
        Album,
    }

    struct TestView {
        list: ListState,
        playback_clicks: Rc<Cell<usize>>,
        title_kind: TitleKind,
    }

    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let playback_clicks = self.playback_clicks.clone();
            let title_kind = self.title_kind;
            div()
                .size_full()
                .relative()
                .child(
                    list(self.list.clone(), move |index, _, _| {
                        let clicks = playback_clicks.clone();
                        let uri = match title_kind {
                            TitleKind::Player if index == 0 => {
                                "spotify:track:0000000000000000000001"
                            }
                            TitleKind::Player => "spotify:episode:0000000000000000000001",
                            TitleKind::Album if index < 2 => "spotify:track:0000000000000000000001",
                            TitleKind::Album => "spotify:episode:0000000000000000000001",
                        };
                        let title = match title_kind {
                            TitleKind::Player => player_track_title("Song", uri).into_any_element(),
                            TitleKind::Album => album_title(
                                "Album",
                                (index == 0).then_some("spotify:album:0000000000000000000001"),
                                uri,
                            )
                            .into_any_element(),
                        };
                        div()
                            .id(("playback-row", index))
                            .h(px(56.0))
                            .w_full()
                            .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .debug_selector(move || format!("title-label-{index}"))
                                    .child(title),
                            )
                            .into_any_element()
                    })
                    .size_full(),
                )
                .child(list_scroll_overlay(
                    self.list.clone(),
                    SmoothScrollState::default(),
                    |_, _| {},
                ))
        }
    }

    #[gpui::test]
    fn title_hover_keeps_hit_target_and_consumes_only_song_clicks(cx: &mut TestAppContext) {
        let clicks = Rc::new(Cell::new(0));
        let (view, cx) = cx.add_window_view(|_, _| TestView {
            list: ListState::new(2, ListAlignment::Top, px(100.0)),
            playback_clicks: clicks.clone(),
            title_kind: TitleKind::Player,
        });
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(200.0), px(112.0)),
            |_, _| view.clone(),
        );
        cx.simulate_mouse_move(point(px(150.0), px(8.0)), None, Default::default());
        let idle_bounds = cx
            .debug_bounds("title-label-0")
            .expect("idle title is rendered");
        cx.simulate_mouse_move(point(px(10.0), px(8.0)), None, Default::default());
        assert_eq!(
            cx.debug_bounds("title-label-0"),
            Some(idle_bounds),
            "hover must not shift the title"
        );
        cx.simulate_click(point(px(10.0), px(8.0)), Default::default());
        assert_eq!(
            clicks.get(),
            0,
            "song title must consume the playback click"
        );
        cx.simulate_mouse_move(point(px(150.0), px(8.0)), None, Default::default());
        assert_eq!(
            cx.debug_bounds("title-label-0"),
            Some(idle_bounds),
            "leaving the title must not shift it"
        );
        cx.simulate_click(point(px(150.0), px(8.0)), Default::default());
        assert_eq!(
            clicks.get(),
            1,
            "space beyond the title must retain row playback"
        );
        cx.simulate_click(point(px(10.0), px(64.0)), Default::default());
        assert_eq!(clicks.get(), 2, "non-song titles must retain row playback");
    }

    #[gpui::test]
    fn album_column_links_stop_row_clicks_and_fall_back_to_track_metadata(cx: &mut TestAppContext) {
        let clicks = Rc::new(Cell::new(0));
        let (view, cx) = cx.add_window_view(|_, _| TestView {
            list: ListState::new(3, ListAlignment::Top, px(100.0)),
            playback_clicks: clicks.clone(),
            title_kind: TitleKind::Album,
        });
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(200.0), px(168.0)),
            |_, _| view.clone(),
        );
        cx.simulate_mouse_move(point(px(150.0), px(8.0)), None, Default::default());
        let idle_bounds = cx
            .debug_bounds("title-label-0")
            .expect("album title is rendered");
        cx.simulate_mouse_move(point(px(10.0), px(8.0)), None, Default::default());
        assert_eq!(
            cx.debug_bounds("title-label-0"),
            Some(idle_bounds),
            "album hover must not shift the title"
        );
        cx.simulate_click(point(px(10.0), px(8.0)), Default::default());
        assert_eq!(
            clicks.get(),
            0,
            "known album link must consume the row click"
        );
        cx.simulate_click(point(px(150.0), px(8.0)), Default::default());
        assert_eq!(
            clicks.get(),
            1,
            "space beyond the album text keeps row playback"
        );
        cx.simulate_click(point(px(10.0), px(64.0)), Default::default());
        assert_eq!(
            clicks.get(),
            1,
            "missing album identity falls back to track metadata"
        );
        cx.simulate_click(point(px(10.0), px(120.0)), Default::default());
        assert_eq!(clicks.get(), 2, "non-song album entries keep row behavior");
    }

    #[test]
    fn only_canonical_song_uris_are_links() {
        assert!(canonical_uri(
            "spotify:track:0000000000000000000001",
            "spotify:track:"
        ));
        for uri in [
            "spotify:episode:0000000000000000000001",
            "spotify:local:artist:album:title:60",
            "spotify:track:example",
            "spotify:track:0000000000000000000001:extra",
            "spotify:user:user:track:0000000000000000000001",
        ] {
            assert!(!canonical_uri(uri, "spotify:track:"), "{uri}");
        }
    }
}
