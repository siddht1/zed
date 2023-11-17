use std::cell::RefCell;
use std::rc::Rc;

use crate::prelude::*;
use crate::{v_stack, Label, List, ListEntry, ListItem, ListSeparator, ListSubHeader};
use gpui::{
    overlay, px, Action, AnchorCorner, AnyElement, Bounds, Dismiss, DispatchPhase, Div,
    FocusHandle, LayoutId, ManagedView, MouseButton, MouseDownEvent, Pixels, Point, Render, View,
    VisualContext, WeakView,
};

pub enum ContextMenuItem<V> {
    Separator(ListSeparator),
    Header(ListSubHeader),
    Entry(
        ListEntry<ContextMenu<V>>,
        Rc<dyn Fn(&mut V, &mut ViewContext<V>)>,
    ),
}

pub struct ContextMenu<V> {
    items: Vec<ContextMenuItem<V>>,
    focus_handle: FocusHandle,
    handle: WeakView<V>,
}

impl<V: Render> ManagedView for ContextMenu<V> {
    fn focus_handle(&self, cx: &gpui::AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl<V: Render> ContextMenu<V> {
    pub fn build(
        cx: &mut ViewContext<V>,
        f: impl FnOnce(Self, &mut ViewContext<Self>) -> Self,
    ) -> View<Self> {
        let handle = cx.view().downgrade();
        cx.build_view(|cx| {
            f(
                Self {
                    handle,
                    items: Default::default(),
                    focus_handle: cx.focus_handle(),
                },
                cx,
            )
        })
    }

    pub fn header(mut self, title: impl Into<SharedString>) -> Self {
        self.items
            .push(ContextMenuItem::Header(ListSubHeader::new(title)));
        self
    }

    pub fn separator(mut self) -> Self {
        self.items.push(ContextMenuItem::Separator(ListSeparator));
        self
    }

    pub fn entry(
        mut self,
        view: ListEntry<Self>,
        on_click: impl Fn(&mut V, &mut ViewContext<V>) + 'static,
    ) -> Self {
        self.items
            .push(ContextMenuItem::Entry(view, Rc::new(on_click)));
        self
    }

    pub fn action(self, view: ListEntry<Self>, action: Box<dyn Action>) -> Self {
        // todo: add the keybindings to the list entry
        self.entry(view, move |_, cx| cx.dispatch_action(action.boxed_clone()))
    }

    pub fn confirm(&mut self, _: &menu::Confirm, cx: &mut ViewContext<Self>) {
        // todo!()
        cx.emit(Dismiss);
    }

    pub fn cancel(&mut self, _: &menu::Cancel, cx: &mut ViewContext<Self>) {
        cx.emit(Dismiss);
    }
}

impl<V: Render> Render for ContextMenu<V> {
    type Element = Div<Self>;

    fn render(&mut self, cx: &mut ViewContext<Self>) -> Self::Element {
        div().elevation_2(cx).flex().flex_row().child(
            v_stack()
                .min_w(px(200.))
                .track_focus(&self.focus_handle)
                .on_mouse_down_out(|this: &mut Self, _, cx| this.cancel(&Default::default(), cx))
                // .on_action(ContextMenu::select_first)
                // .on_action(ContextMenu::select_last)
                // .on_action(ContextMenu::select_next)
                // .on_action(ContextMenu::select_prev)
                .on_action(ContextMenu::confirm)
                .on_action(ContextMenu::cancel)
                .flex_none()
                // .bg(cx.theme().colors().elevated_surface_background)
                // .border()
                // .border_color(cx.theme().colors().border)
                .child(List::new(
                    self.items
                        .iter()
                        .map(|item| match item {
                            ContextMenuItem::Separator(separator) => {
                                ListItem::Separator(separator.clone())
                            }
                            ContextMenuItem::Header(header) => ListItem::Header(header.clone()),
                            ContextMenuItem::Entry(entry, callback) => {
                                let callback = callback.clone();
                                let handle = self.handle.clone();
                                ListItem::Entry(entry.clone().on_click(move |this, cx| {
                                    handle.update(cx, |view, cx| callback(view, cx)).ok();
                                    cx.emit(Dismiss);
                                }))
                            }
                        })
                        .collect(),
                )),
        )
    }
}

pub struct MenuHandle<V: 'static, M: ManagedView> {
    id: Option<ElementId>,
    child_builder: Option<Box<dyn FnOnce(bool) -> AnyElement<V> + 'static>>,
    menu_builder: Option<Rc<dyn Fn(&mut V, &mut ViewContext<V>) -> View<M> + 'static>>,

    anchor: Option<AnchorCorner>,
    attach: Option<AnchorCorner>,
}

impl<V: 'static, M: ManagedView> MenuHandle<V, M> {
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn menu(mut self, f: impl Fn(&mut V, &mut ViewContext<V>) -> View<M> + 'static) -> Self {
        self.menu_builder = Some(Rc::new(f));
        self
    }

    pub fn child<R: Component<V>>(mut self, f: impl FnOnce(bool) -> R + 'static) -> Self {
        self.child_builder = Some(Box::new(|b| f(b).render()));
        self
    }

    /// anchor defines which corner of the menu to anchor to the attachment point
    /// (by default the cursor position, but see attach)
    pub fn anchor(mut self, anchor: AnchorCorner) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// attach defines which corner of the handle to attach the menu's anchor to
    pub fn attach(mut self, attach: AnchorCorner) -> Self {
        self.attach = Some(attach);
        self
    }
}

pub fn menu_handle<V: 'static, M: ManagedView>() -> MenuHandle<V, M> {
    MenuHandle {
        id: None,
        child_builder: None,
        menu_builder: None,
        anchor: None,
        attach: None,
    }
}

pub struct MenuHandleState<V, M> {
    menu: Rc<RefCell<Option<View<M>>>>,
    position: Rc<RefCell<Point<Pixels>>>,
    child_layout_id: Option<LayoutId>,
    child_element: Option<AnyElement<V>>,
    menu_element: Option<AnyElement<V>>,
}
impl<V: 'static, M: ManagedView> Element<V> for MenuHandle<V, M> {
    type ElementState = MenuHandleState<V, M>;

    fn element_id(&self) -> Option<gpui::ElementId> {
        Some(self.id.clone().expect("menu_handle must have an id()"))
    }

    fn layout(
        &mut self,
        view_state: &mut V,
        element_state: Option<Self::ElementState>,
        cx: &mut crate::ViewContext<V>,
    ) -> (gpui::LayoutId, Self::ElementState) {
        let (menu, position) = if let Some(element_state) = element_state {
            (element_state.menu, element_state.position)
        } else {
            (Rc::default(), Rc::default())
        };

        let mut menu_layout_id = None;

        let menu_element = menu.borrow_mut().as_mut().map(|menu| {
            let mut overlay = overlay::<V>().snap_to_window();
            if let Some(anchor) = self.anchor {
                overlay = overlay.anchor(anchor);
            }
            overlay = overlay.position(*position.borrow());

            let mut view = overlay.child(menu.clone()).render();
            menu_layout_id = Some(view.layout(view_state, cx));
            view
        });

        let mut child_element = self
            .child_builder
            .take()
            .map(|child_builder| (child_builder)(menu.borrow().is_some()));

        let child_layout_id = child_element
            .as_mut()
            .map(|child_element| child_element.layout(view_state, cx));

        let layout_id = cx.request_layout(
            &gpui::Style::default(),
            menu_layout_id.into_iter().chain(child_layout_id),
        );

        (
            layout_id,
            MenuHandleState {
                menu,
                position,
                child_element,
                child_layout_id,
                menu_element,
            },
        )
    }

    fn paint(
        &mut self,
        bounds: Bounds<gpui::Pixels>,
        view_state: &mut V,
        element_state: &mut Self::ElementState,
        cx: &mut crate::ViewContext<V>,
    ) {
        if let Some(child) = element_state.child_element.as_mut() {
            child.paint(view_state, cx);
        }

        if let Some(menu) = element_state.menu_element.as_mut() {
            menu.paint(view_state, cx);
            return;
        }

        let Some(builder) = self.menu_builder.clone() else {
            return;
        };
        let menu = element_state.menu.clone();
        let position = element_state.position.clone();
        let attach = self.attach.clone();
        let child_layout_id = element_state.child_layout_id.clone();

        cx.on_mouse_event(move |view_state, event: &MouseDownEvent, phase, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Right
                && bounds.contains_point(&event.position)
            {
                cx.stop_propagation();
                cx.prevent_default();

                let new_menu = (builder)(view_state, cx);
                let menu2 = menu.clone();
                cx.subscribe(&new_menu, move |this, modal, e, cx| match e {
                    &Dismiss => {
                        *menu2.borrow_mut() = None;
                        cx.notify();
                    }
                })
                .detach();
                cx.focus_view(&new_menu);
                *menu.borrow_mut() = Some(new_menu);

                *position.borrow_mut() = if attach.is_some() && child_layout_id.is_some() {
                    attach
                        .unwrap()
                        .corner(cx.layout_bounds(child_layout_id.unwrap()))
                } else {
                    cx.mouse_position()
                };
                cx.notify();
            }
        });
    }
}

impl<V: 'static, M: ManagedView> Component<V> for MenuHandle<V, M> {
    fn render(self) -> AnyElement<V> {
        AnyElement::new(self)
    }
}

#[cfg(feature = "stories")]
pub use stories::*;

#[cfg(feature = "stories")]
mod stories {
    use super::*;
    use crate::story::Story;
    use gpui::{actions, Div, Render};

    actions!(PrintCurrentDate, PrintBestFood);

    fn build_menu<V: Render>(
        cx: &mut ViewContext<V>,
        header: impl Into<SharedString>,
    ) -> View<ContextMenu<V>> {
        let handle = cx.view().clone();
        ContextMenu::build(cx, |menu, _| {
            menu.header(header)
                .separator()
                .entry(ListEntry::new(Label::new("Print current time")), |v, cx| {
                    println!("dispatching PrintCurrentTime action");
                    cx.dispatch_action(PrintCurrentDate.boxed_clone())
                })
                .entry(ListEntry::new(Label::new("Print best food")), |v, cx| {
                    cx.dispatch_action(PrintBestFood.boxed_clone())
                })
        })
    }

    pub struct ContextMenuStory;

    impl Render for ContextMenuStory {
        type Element = Div<Self>;

        fn render(&mut self, cx: &mut ViewContext<Self>) -> Self::Element {
            Story::container(cx)
                .on_action(|_, _: &PrintCurrentDate, _| {
                    println!("printing unix time!");
                    if let Ok(unix_time) = std::time::UNIX_EPOCH.elapsed() {
                        println!("Current Unix time is {:?}", unix_time.as_secs());
                    }
                })
                .on_action(|_, _: &PrintBestFood, _| {
                    println!("burrito");
                })
                .flex()
                .flex_row()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .justify_between()
                        .child(
                            menu_handle()
                                .id("test2")
                                .child(|is_open| {
                                    Label::new(if is_open {
                                        "TOP LEFT"
                                    } else {
                                        "RIGHT CLICK ME"
                                    })
                                    .render()
                                })
                                .menu(move |_, cx| build_menu(cx, "top left")),
                        )
                        .child(
                            menu_handle()
                                .id("test1")
                                .child(|is_open| {
                                    Label::new(if is_open {
                                        "BOTTOM LEFT"
                                    } else {
                                        "RIGHT CLICK ME"
                                    })
                                    .render()
                                })
                                .anchor(AnchorCorner::BottomLeft)
                                .attach(AnchorCorner::TopLeft)
                                .menu(move |_, cx| build_menu(cx, "bottom left")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .justify_between()
                        .child(
                            menu_handle()
                                .id("test3")
                                .child(|is_open| {
                                    Label::new(if is_open {
                                        "TOP RIGHT"
                                    } else {
                                        "RIGHT CLICK ME"
                                    })
                                    .render()
                                })
                                .anchor(AnchorCorner::TopRight)
                                .menu(move |_, cx| build_menu(cx, "top right")),
                        )
                        .child(
                            menu_handle()
                                .id("test4")
                                .child(|is_open| {
                                    Label::new(if is_open {
                                        "BOTTOM RIGHT"
                                    } else {
                                        "RIGHT CLICK ME"
                                    })
                                    .render()
                                })
                                .anchor(AnchorCorner::BottomRight)
                                .attach(AnchorCorner::TopRight)
                                .menu(move |_, cx| build_menu(cx, "bottom right")),
                        ),
                )
        }
    }
}