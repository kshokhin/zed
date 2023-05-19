use crate::{StatusItemView, Workspace};
use context_menu::{ContextMenu, ContextMenuItem};
use gpui::{
    elements::*, impl_actions, platform::CursorStyle, platform::MouseButton, AnyViewHandle,
    AppContext, Axis, Entity, Subscription, View, ViewContext, ViewHandle, WeakViewHandle,
    WindowContext,
};
use serde::Deserialize;
use settings::Settings;
use std::rc::Rc;

pub trait Panel: View {
    fn position(&self, cx: &WindowContext) -> DockPosition;
    fn position_is_valid(&self, position: DockPosition) -> bool;
    fn set_position(&mut self, position: DockPosition, cx: &mut ViewContext<Self>);
    fn default_size(&self, cx: &WindowContext) -> f32;
    fn icon_path(&self) -> &'static str;
    fn icon_tooltip(&self) -> String;
    fn icon_label(&self, _: &WindowContext) -> Option<String> {
        None
    }
    fn should_change_position_on_event(_: &Self::Event) -> bool;
    fn should_zoom_in_on_event(_: &Self::Event) -> bool;
    fn should_zoom_out_on_event(_: &Self::Event) -> bool;
    fn is_zoomed(&self, cx: &WindowContext) -> bool;
    fn set_zoomed(&mut self, zoomed: bool, cx: &mut ViewContext<Self>);
    fn set_active(&mut self, active: bool, cx: &mut ViewContext<Self>);
    fn should_activate_on_event(_: &Self::Event) -> bool;
    fn should_close_on_event(_: &Self::Event) -> bool;
    fn has_focus(&self, cx: &WindowContext) -> bool;
    fn is_focus_event(_: &Self::Event) -> bool;
}

pub trait PanelHandle {
    fn id(&self) -> usize;
    fn position(&self, cx: &WindowContext) -> DockPosition;
    fn position_is_valid(&self, position: DockPosition, cx: &WindowContext) -> bool;
    fn set_position(&self, position: DockPosition, cx: &mut WindowContext);
    fn is_zoomed(&self, cx: &WindowContext) -> bool;
    fn set_zoomed(&self, zoomed: bool, cx: &mut WindowContext);
    fn set_active(&self, active: bool, cx: &mut WindowContext);
    fn default_size(&self, cx: &WindowContext) -> f32;
    fn icon_path(&self, cx: &WindowContext) -> &'static str;
    fn icon_tooltip(&self, cx: &WindowContext) -> String;
    fn icon_label(&self, cx: &WindowContext) -> Option<String>;
    fn has_focus(&self, cx: &WindowContext) -> bool;
    fn as_any(&self) -> &AnyViewHandle;
}

impl<T> PanelHandle for ViewHandle<T>
where
    T: Panel,
{
    fn id(&self) -> usize {
        self.id()
    }

    fn position(&self, cx: &WindowContext) -> DockPosition {
        self.read(cx).position(cx)
    }

    fn position_is_valid(&self, position: DockPosition, cx: &WindowContext) -> bool {
        self.read(cx).position_is_valid(position)
    }

    fn set_position(&self, position: DockPosition, cx: &mut WindowContext) {
        self.update(cx, |this, cx| this.set_position(position, cx))
    }

    fn default_size(&self, cx: &WindowContext) -> f32 {
        self.read(cx).default_size(cx)
    }

    fn is_zoomed(&self, cx: &WindowContext) -> bool {
        self.read(cx).is_zoomed(cx)
    }

    fn set_zoomed(&self, zoomed: bool, cx: &mut WindowContext) {
        self.update(cx, |this, cx| this.set_zoomed(zoomed, cx))
    }

    fn set_active(&self, active: bool, cx: &mut WindowContext) {
        self.update(cx, |this, cx| this.set_active(active, cx))
    }

    fn icon_path(&self, cx: &WindowContext) -> &'static str {
        self.read(cx).icon_path()
    }

    fn icon_tooltip(&self, cx: &WindowContext) -> String {
        self.read(cx).icon_tooltip()
    }

    fn icon_label(&self, cx: &WindowContext) -> Option<String> {
        self.read(cx).icon_label(cx)
    }

    fn has_focus(&self, cx: &WindowContext) -> bool {
        self.read(cx).has_focus(cx)
    }

    fn as_any(&self) -> &AnyViewHandle {
        self
    }
}

impl From<&dyn PanelHandle> for AnyViewHandle {
    fn from(val: &dyn PanelHandle) -> Self {
        val.as_any().clone()
    }
}

pub struct Dock {
    position: DockPosition,
    panel_entries: Vec<PanelEntry>,
    is_open: bool,
    active_panel_index: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum DockPosition {
    Left,
    Bottom,
    Right,
}

impl DockPosition {
    fn to_label(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Bottom => "bottom",
            Self::Right => "right",
        }
    }

    fn to_resize_handle_side(self) -> HandleSide {
        match self {
            Self::Left => HandleSide::Right,
            Self::Bottom => HandleSide::Top,
            Self::Right => HandleSide::Left,
        }
    }

    pub fn axis(&self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Horizontal,
            Self::Bottom => Axis::Vertical,
        }
    }
}

struct PanelEntry {
    panel: Rc<dyn PanelHandle>,
    size: f32,
    context_menu: ViewHandle<ContextMenu>,
    _subscriptions: [Subscription; 2],
}

pub struct PanelButtons {
    dock: ViewHandle<Dock>,
    workspace: WeakViewHandle<Workspace>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TogglePanel {
    pub dock_position: DockPosition,
    pub panel_index: usize,
}

impl_actions!(workspace, [TogglePanel]);

impl Dock {
    pub fn new(position: DockPosition) -> Self {
        Self {
            position,
            panel_entries: Default::default(),
            active_panel_index: 0,
            is_open: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn has_focus(&self, cx: &WindowContext) -> bool {
        self.active_panel()
            .map_or(false, |panel| panel.has_focus(cx))
    }

    pub fn panel_index_for_type<T: Panel>(&self) -> Option<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel.as_any().is::<T>())
    }

    pub fn panel_index_for_ui_name(&self, ui_name: &str, cx: &AppContext) -> Option<usize> {
        self.panel_entries.iter().position(|entry| {
            let panel = entry.panel.as_any();
            cx.view_ui_name(panel.window_id(), panel.id()) == Some(ui_name)
        })
    }

    pub fn active_panel_index(&self) -> usize {
        self.active_panel_index
    }

    pub fn set_open(&mut self, open: bool, cx: &mut ViewContext<Self>) {
        if open != self.is_open {
            self.is_open = open;
            if let Some(active_panel) = self.panel_entries.get(self.active_panel_index) {
                active_panel.panel.set_active(open, cx);
            }

            cx.notify();
        }
    }

    pub fn toggle_open(&mut self, cx: &mut ViewContext<Self>) {
        self.set_open(!self.is_open, cx);
        cx.notify();
    }

    pub fn set_panel_zoomed(
        &mut self,
        panel: &AnyViewHandle,
        zoomed: bool,
        cx: &mut ViewContext<Self>,
    ) {
        for entry in &mut self.panel_entries {
            if entry.panel.as_any() == panel {
                if zoomed != entry.panel.is_zoomed(cx) {
                    entry.panel.set_zoomed(zoomed, cx);
                }
            } else if entry.panel.is_zoomed(cx) {
                entry.panel.set_zoomed(false, cx);
            }
        }

        cx.notify();
    }

    pub fn zoom_out(&mut self, cx: &mut ViewContext<Self>) {
        for entry in &mut self.panel_entries {
            if entry.panel.is_zoomed(cx) {
                entry.panel.set_zoomed(false, cx);
            }
        }
    }

    pub fn add_panel<T: Panel>(&mut self, panel: ViewHandle<T>, cx: &mut ViewContext<Self>) {
        let subscriptions = [
            cx.observe(&panel, |_, _, cx| cx.notify()),
            cx.subscribe(&panel, |this, panel, event, cx| {
                if T::should_activate_on_event(event) {
                    if let Some(ix) = this
                        .panel_entries
                        .iter()
                        .position(|entry| entry.panel.id() == panel.id())
                    {
                        this.set_open(true, cx);
                        this.activate_panel(ix, cx);
                        cx.focus(&panel);
                    }
                } else if T::should_close_on_event(event)
                    && this.active_panel().map_or(false, |p| p.id() == panel.id())
                {
                    this.set_open(false, cx);
                }
            }),
        ];

        let dock_view_id = cx.view_id();
        let size = panel.default_size(cx);
        self.panel_entries.push(PanelEntry {
            panel: Rc::new(panel),
            size,
            context_menu: cx.add_view(|cx| {
                let mut menu = ContextMenu::new(dock_view_id, cx);
                menu.set_position_mode(OverlayPositionMode::Local);
                menu
            }),
            _subscriptions: subscriptions,
        });
        cx.notify()
    }

    pub fn remove_panel<T: Panel>(&mut self, panel: &ViewHandle<T>, cx: &mut ViewContext<Self>) {
        if let Some(panel_ix) = self
            .panel_entries
            .iter()
            .position(|entry| entry.panel.id() == panel.id())
        {
            if panel_ix == self.active_panel_index {
                self.active_panel_index = 0;
                self.set_open(false, cx);
            } else if panel_ix < self.active_panel_index {
                self.active_panel_index -= 1;
            }
            self.panel_entries.remove(panel_ix);
            cx.notify();
        }
    }

    pub fn panels_len(&self) -> usize {
        self.panel_entries.len()
    }

    pub fn activate_panel(&mut self, panel_ix: usize, cx: &mut ViewContext<Self>) {
        if panel_ix != self.active_panel_index {
            if let Some(active_panel) = self.panel_entries.get(self.active_panel_index) {
                active_panel.panel.set_active(false, cx);
            }

            self.active_panel_index = panel_ix;
            if let Some(active_panel) = self.panel_entries.get(self.active_panel_index) {
                active_panel.panel.set_active(true, cx);
            }

            cx.notify();
        }
    }

    pub fn active_panel(&self) -> Option<&Rc<dyn PanelHandle>> {
        let entry = self.active_entry()?;
        Some(&entry.panel)
    }

    fn active_entry(&self) -> Option<&PanelEntry> {
        if self.is_open {
            self.panel_entries.get(self.active_panel_index)
        } else {
            None
        }
    }

    pub fn zoomed_panel(&self, cx: &WindowContext) -> Option<Rc<dyn PanelHandle>> {
        let entry = self.active_entry()?;
        if entry.panel.is_zoomed(cx) {
            Some(entry.panel.clone())
        } else {
            None
        }
    }

    pub fn panel_size(&self, panel: &dyn PanelHandle) -> Option<f32> {
        self.panel_entries
            .iter()
            .find(|entry| entry.panel.id() == panel.id())
            .map(|entry| entry.size)
    }

    pub fn resize_panel(&mut self, panel: &dyn PanelHandle, size: f32) {
        let entry = self
            .panel_entries
            .iter_mut()
            .find(|entry| entry.panel.id() == panel.id());
        if let Some(entry) = entry {
            entry.size = size;
        }
    }

    pub fn active_panel_size(&self) -> Option<f32> {
        if self.is_open {
            self.panel_entries
                .get(self.active_panel_index)
                .map(|entry| entry.size)
        } else {
            None
        }
    }

    pub fn resize_active_panel(&mut self, size: f32, cx: &mut ViewContext<Self>) {
        if let Some(entry) = self.panel_entries.get_mut(self.active_panel_index) {
            entry.size = size;
            cx.notify();
        }
    }

    pub fn render_placeholder(&self, cx: &WindowContext) -> AnyElement<Workspace> {
        if let Some(active_entry) = self.active_entry() {
            let style = &cx.global::<Settings>().theme.workspace.dock;
            Empty::new()
                .into_any()
                .contained()
                .with_style(style.container)
                .resizable(
                    self.position.to_resize_handle_side(),
                    active_entry.size,
                    |_, _, _| {},
                )
                .into_any()
        } else {
            Empty::new().into_any()
        }
    }
}

impl Entity for Dock {
    type Event = ();
}

impl View for Dock {
    fn ui_name() -> &'static str {
        "Dock"
    }

    fn render(&mut self, cx: &mut ViewContext<Self>) -> AnyElement<Self> {
        if let Some(active_entry) = self.active_entry() {
            let style = &cx.global::<Settings>().theme.workspace.dock;
            ChildView::new(active_entry.panel.as_any(), cx)
                .contained()
                .with_style(style.container)
                .resizable(
                    self.position.to_resize_handle_side(),
                    active_entry.size,
                    |dock: &mut Self, size, cx| dock.resize_active_panel(size, cx),
                )
                .into_any()
        } else {
            Empty::new().into_any()
        }
    }
}

impl PanelButtons {
    pub fn new(
        dock: ViewHandle<Dock>,
        workspace: WeakViewHandle<Workspace>,
        cx: &mut ViewContext<Self>,
    ) -> Self {
        cx.observe(&dock, |_, _, cx| cx.notify()).detach();
        Self { dock, workspace }
    }
}

impl Entity for PanelButtons {
    type Event = ();
}

impl View for PanelButtons {
    fn ui_name() -> &'static str {
        "PanelButtons"
    }

    fn render(&mut self, cx: &mut ViewContext<Self>) -> AnyElement<Self> {
        let theme = &cx.global::<Settings>().theme;
        let tooltip_style = theme.tooltip.clone();
        let theme = &theme.workspace.status_bar.panel_buttons;
        let button_style = theme.button.clone();
        let dock = self.dock.read(cx);
        let active_ix = dock.active_panel_index;
        let is_open = dock.is_open;
        let dock_position = dock.position;
        let group_style = match dock_position {
            DockPosition::Left => theme.group_left,
            DockPosition::Bottom => theme.group_bottom,
            DockPosition::Right => theme.group_right,
        };
        let menu_corner = match dock_position {
            DockPosition::Left => AnchorCorner::BottomLeft,
            DockPosition::Bottom | DockPosition::Right => AnchorCorner::BottomRight,
        };

        let panels = dock
            .panel_entries
            .iter()
            .map(|item| (item.panel.clone(), item.context_menu.clone()))
            .collect::<Vec<_>>();
        Flex::row()
            .with_children(
                panels
                    .into_iter()
                    .enumerate()
                    .map(|(ix, (view, context_menu))| {
                        let action = TogglePanel {
                            dock_position,
                            panel_index: ix,
                        };

                        Stack::new()
                            .with_child(
                                MouseEventHandler::<Self, _>::new(ix, cx, |state, cx| {
                                    let is_active = is_open && ix == active_ix;
                                    let style = button_style.style_for(state, is_active);
                                    Flex::row()
                                        .with_child(
                                            Svg::new(view.icon_path(cx))
                                                .with_color(style.icon_color)
                                                .constrained()
                                                .with_width(style.icon_size)
                                                .aligned(),
                                        )
                                        .with_children(if let Some(label) = view.icon_label(cx) {
                                            Some(
                                                Label::new(label, style.label.text.clone())
                                                    .contained()
                                                    .with_style(style.label.container)
                                                    .aligned(),
                                            )
                                        } else {
                                            None
                                        })
                                        .constrained()
                                        .with_height(style.icon_size)
                                        .contained()
                                        .with_style(style.container)
                                })
                                .with_cursor_style(CursorStyle::PointingHand)
                                .on_click(MouseButton::Left, {
                                    let action = action.clone();
                                    move |_, this, cx| {
                                        if let Some(workspace) = this.workspace.upgrade(cx) {
                                            let action = action.clone();
                                            cx.window_context().defer(move |cx| {
                                                workspace.update(cx, |workspace, cx| {
                                                    workspace.toggle_panel(&action, cx)
                                                });
                                            });
                                        }
                                    }
                                })
                                .on_click(MouseButton::Right, {
                                    let view = view.clone();
                                    let menu = context_menu.clone();
                                    move |_, _, cx| {
                                        const POSITIONS: [DockPosition; 3] = [
                                            DockPosition::Left,
                                            DockPosition::Right,
                                            DockPosition::Bottom,
                                        ];

                                        menu.update(cx, |menu, cx| {
                                            let items = POSITIONS
                                                .into_iter()
                                                .filter(|position| {
                                                    *position != dock_position
                                                        && view.position_is_valid(*position, cx)
                                                })
                                                .map(|position| {
                                                    let view = view.clone();
                                                    ContextMenuItem::handler(
                                                        format!("Dock {}", position.to_label()),
                                                        move |cx| view.set_position(position, cx),
                                                    )
                                                })
                                                .collect();
                                            menu.show(Default::default(), menu_corner, items, cx);
                                        })
                                    }
                                })
                                .with_tooltip::<Self>(
                                    ix,
                                    view.icon_tooltip(cx),
                                    Some(Box::new(action)),
                                    tooltip_style.clone(),
                                    cx,
                                ),
                            )
                            .with_child(ChildView::new(&context_menu, cx))
                    }),
            )
            .contained()
            .with_style(group_style)
            .into_any()
    }
}

impl StatusItemView for PanelButtons {
    fn set_active_pane_item(
        &mut self,
        _: Option<&dyn crate::ItemHandle>,
        _: &mut ViewContext<Self>,
    ) {
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use gpui::Entity;

    pub enum TestPanelEvent {
        PositionChanged,
        Activated,
        Closed,
    }

    pub struct TestPanel {
        pub position: DockPosition,
    }

    impl Entity for TestPanel {
        type Event = TestPanelEvent;
    }

    impl View for TestPanel {
        fn ui_name() -> &'static str {
            "TestPanel"
        }

        fn render(&mut self, _: &mut ViewContext<'_, '_, Self>) -> AnyElement<Self> {
            Empty::new().into_any()
        }
    }

    impl Panel for TestPanel {
        fn position(&self, _: &gpui::WindowContext) -> super::DockPosition {
            self.position
        }

        fn position_is_valid(&self, _: super::DockPosition) -> bool {
            true
        }

        fn set_position(&mut self, position: DockPosition, cx: &mut ViewContext<Self>) {
            self.position = position;
            cx.emit(TestPanelEvent::PositionChanged);
        }

        fn is_zoomed(&self, _: &WindowContext) -> bool {
            unimplemented!()
        }

        fn set_zoomed(&mut self, _zoomed: bool, _cx: &mut ViewContext<Self>) {
            unimplemented!()
        }

        fn set_active(&mut self, _active: bool, _cx: &mut ViewContext<Self>) {
            unimplemented!()
        }

        fn default_size(&self, _: &WindowContext) -> f32 {
            match self.position.axis() {
                Axis::Horizontal => 300.,
                Axis::Vertical => 200.,
            }
        }

        fn icon_path(&self) -> &'static str {
            "icons/test_panel.svg"
        }

        fn icon_tooltip(&self) -> String {
            "Test Panel".into()
        }

        fn should_change_position_on_event(event: &Self::Event) -> bool {
            matches!(event, TestPanelEvent::PositionChanged)
        }

        fn should_zoom_in_on_event(_: &Self::Event) -> bool {
            false
        }

        fn should_zoom_out_on_event(_: &Self::Event) -> bool {
            false
        }

        fn should_activate_on_event(event: &Self::Event) -> bool {
            matches!(event, TestPanelEvent::Activated)
        }

        fn should_close_on_event(event: &Self::Event) -> bool {
            matches!(event, TestPanelEvent::Closed)
        }

        fn has_focus(&self, _cx: &WindowContext) -> bool {
            unimplemented!()
        }

        fn is_focus_event(_: &Self::Event) -> bool {
            unimplemented!()
        }
    }
}
