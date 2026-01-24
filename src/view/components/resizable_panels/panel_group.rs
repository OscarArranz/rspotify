use gpui::{App, ElementId, Entity, IntoElement, ParentElement, RenderOnce, Window, div};

use crate::view::components::resizable_panels::panel::Panel;

struct ResizablePanelGroupState;

pub struct ResizablePanelGroup {
    id: ElementId,
    state: Option<Entity<ResizablePanelGroupState>>,
    children: Vec<Panel>,
}

impl ResizablePanelGroup {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            state: None,
            children: vec![],
        }
    }
}

impl RenderOnce for ResizablePanelGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self
            .state
            .unwrap_or(window.use_keyed_state(self.id, cx, |_, _| ResizablePanelGroupState {}));

        div().children(
            self.children
                .into_iter()
                .enumerate()
                .map(|(ix, mut panel)| {
                    // panel.panel_ix = ix;
                    // panel.axis = self.axis;
                    // panel.state = Some(state.clone());
                    panel
                }),
        )
    }
}
