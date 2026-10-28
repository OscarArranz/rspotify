use gpui::{
    InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div, rgb,
};

use crate::lib::spotify::ArtistCredit;
use crate::view::screens::search::open_artist_profile;

pub(crate) fn artist_credits(credits: &[ArtistCredit], fallback: &str) -> impl IntoElement {
    let mut row = div()
        .flex()
        .items_center()
        .overflow_hidden()
        .text_color(rgb(0xb3b3b3));
    if credits.is_empty() {
        return row.child(fallback.to_string());
    }
    for (index, credit) in credits.iter().enumerate() {
        if index > 0 {
            row = row.child(", ");
        }
        let name = credit.name.clone();
        let uri = credit.uri.clone();
        if !uri.starts_with("spotify:artist:") {
            row = row.child(name);
            continue;
        }
        row = row.child(
            div()
                .id(("artist-credit", index))
                .flex_shrink_0()
                .cursor_pointer()
                .border_b_1()
                .border_color(gpui::transparent_black())
                .hover(|style| style.border_color(rgb(0xffffff)).text_color(rgb(0xffffff)))
                .child(name.clone())
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    open_artist_profile(&uri, &name, None, cx);
                }),
        );
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::components::smooth_scroll::{SmoothScrollState, list_scroll_overlay};
    use gpui::{
        Context, ListAlignment, ListState, Render, TestAppContext, Window, list, point, px, size,
    };
    use std::{cell::Cell, rc::Rc};

    struct TestView {
        list: ListState,
        row_clicks: Rc<Cell<usize>>,
    }

    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let row_clicks = self.row_clicks.clone();
            div()
                .size_full()
                .relative()
                .child(
                    list(self.list.clone(), move |_, _, _| {
                        let clicks = row_clicks.clone();
                        div()
                            .id("track")
                            .h(px(56.0))
                            .w_full()
                            .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
                            .child(div().text_size(px(13.0)).child(artist_credits(
                                &[ArtistCredit {
                                    name: "Artist".into(),
                                    uri: "spotify:artist:0000000000000000000001".into(),
                                }],
                                "",
                            )))
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
    fn playlist_artist_click_does_not_activate_track(cx: &mut TestAppContext) {
        let clicks = Rc::new(Cell::new(0));
        let (view, cx) = cx.add_window_view(|_, _| TestView {
            list: ListState::new(1, ListAlignment::Top, px(100.0)),
            row_clicks: clicks.clone(),
        });
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(200.0), px(100.0)),
            |_, _| view.clone(),
        );
        cx.simulate_click(point(px(10.0), px(8.0)), Default::default());
        assert_eq!(clicks.get(), 0, "artist link should consume the click");
    }
}
