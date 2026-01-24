use gpui::{App, IntoElement, RenderOnce, Window, div};

pub struct Panel;

impl Panel {
    fn new() -> Self {
        Self {}
    }
}

impl RenderOnce for Panel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
    }
}
