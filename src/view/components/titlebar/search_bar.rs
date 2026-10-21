//! Search input in the title bar.

use gpui::Subscription;
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    Window, div, px, rgb, svg,
};
use gpui_component::input::{Input, InputEvent, InputState};

use crate::router::StackRouterHandle;
use crate::state::AppState;
use crate::view::screens::search::{SearchScreen, SearchTab};

pub(super) struct SearchBar {
    input: Entity<InputState>,
    _subscription: Subscription,
}

impl SearchBar {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("What do you want to play?"));
        let _subscription = cx.subscribe(&input, |_, input, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let query = input.read(cx).value().to_string().trim().to_string();
                if query.is_empty() {
                    return;
                }

                let Some(spotify) = cx
                    .try_global::<AppState>()
                    .and_then(AppState::spotify_client)
                else {
                    return;
                };

                if let Some(router) = cx.try_global::<StackRouterHandle>().cloned() {
                    let screen = cx.new(|cx| SearchScreen::new(query, SearchTab::All, spotify, cx));
                    router.push(screen.into(), cx);
                }
            }
        });

        Self {
            input,
            _subscription,
        }
    }
}

impl Render for SearchBar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("search-bar")
            .flex()
            .items_center()
            .h(px(48.0))
            .w(px(475.0))
            .px(px(12.0))
            .bg(rgb(0x242424))
            .rounded(px(24.0))
            .border_1()
            .border_color(rgb(0x3e3e3e))
            .child(
                Input::new(&self.input)
                    .prefix(
                        svg()
                            .path("icons/search.svg")
                            .size(px(18.0))
                            .text_color(rgb(0xb3b3b3)),
                    )
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .cleanable(true)
                    .w_full()
                    .text_color(rgb(0xffffff)),
            )
    }
}
