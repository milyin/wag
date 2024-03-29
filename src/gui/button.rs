use std::borrow::Cow;

use super::{attach, Text, TextParams};
use super::{Background, BackgroundParams, LayerStack, LayerStackParams, Panel, PanelEvent};
use async_event_streams::{
    EventBox, EventSink, EventSinkExt, EventSource, EventStream, EventStreams,
};
use async_event_streams_derive::{self, EventSink};
use async_std::sync::Arc;
use async_std::sync::RwLock;
use async_trait::async_trait;
use futures::task::Spawn;
use typed_builder::TypedBuilder;
use windows::UI::Composition::Visual;
use windows::UI::{
    Color, Colors,
    Composition::{Compositor, ContainerVisual},
};
use winit::event::{ElementState, MouseButton};

#[derive(PartialEq, Clone, Debug)]
pub enum ButtonEvent {
    Press,
    Release(bool),
}

struct Core {
    skin: Arc<dyn ButtonSkin>,
    pressed: bool,
    button_events: Arc<EventStreams<ButtonEvent>>,
}

#[derive(EventSink)]
#[event_sink(event=PanelEvent)]
pub struct Button {
    container: ContainerVisual,
    core: RwLock<Core>,
    panel_events: EventStreams<PanelEvent>,
    button_events: Arc<EventStreams<ButtonEvent>>,
    id: Arc<()>,
}

#[derive(TypedBuilder)]
pub struct ButtonParams {
    compositor: Compositor,
    #[builder(setter(transform = |skin: impl ButtonSkin + 'static | Arc::new(skin) as Arc<dyn ButtonSkin>))]
    skin: Arc<dyn ButtonSkin>,
}

impl TryFrom<ButtonParams> for Button {
    type Error = crate::Error;

    fn try_from(value: ButtonParams) -> crate::Result<Self> {
        let container = value.compositor.CreateContainerVisual()?;
        let skin = value.skin;
        attach(&container, &*skin)?;
        let button_events = Arc::new(EventStreams::new());
        let core = RwLock::new(Core {
            skin,
            pressed: false,
            button_events: button_events.clone(),
        });
        Ok(Button {
            container,
            core,
            panel_events: EventStreams::new(),
            button_events,
            id: Arc::new(()),
        })
    }
}

impl TryFrom<ButtonParams> for Arc<Button> {
    type Error = crate::Error;

    fn try_from(value: ButtonParams) -> crate::Result<Self> {
        Ok(Arc::new(value.try_into()?))
    }
}

impl Core {
    async fn press(&mut self, source: Option<Arc<EventBox>>) -> crate::Result<()> {
        self.pressed = true;
        let event = ButtonEvent::Press;
        self.skin.on_event_ref(&event, source.clone()).await?;
        self.button_events.send_event(event, source).await;
        Ok(())
    }
    async fn release(&mut self, in_slot: bool, source: Option<Arc<EventBox>>) -> crate::Result<()> {
        self.pressed = false;
        let event = ButtonEvent::Release(in_slot);
        self.skin.on_event_ref(&event, source.clone()).await?;
        self.button_events.send_event(event, source).await;
        Ok(())
    }
    fn is_pressed(&self) -> bool {
        self.pressed
    }
    fn skin_panel(&self) -> Arc<dyn ButtonSkin> {
        self.skin.clone()
    }
}

impl EventSource<ButtonEvent> for Button {
    fn event_stream(&self) -> EventStream<ButtonEvent> {
        self.button_events.create_event_stream()
    }
}

impl EventSource<PanelEvent> for Button {
    fn event_stream(&self) -> EventStream<PanelEvent> {
        self.panel_events.create_event_stream()
    }
}

#[async_trait]
impl EventSinkExt<PanelEvent> for Button {
    type Error = crate::Error;
    async fn on_event<'a>(
        &'a self,
        event: Cow<'a, PanelEvent>,
        source: Option<Arc<EventBox>>,
    ) -> crate::Result<()> {
        let skin = self.core.read().await.skin_panel();
        skin.on_event_ref(event.as_ref(), source.clone()).await?;
        self.panel_events
            .send_event(event.clone().into_owned(), source.clone())
            .await;
        match event.as_ref() {
            PanelEvent::MouseInput {
                in_slot,
                state,
                button,
            } => {
                if *button == MouseButton::Left {
                    if *state == ElementState::Pressed {
                        if *in_slot {
                            self.core.write().await.press(source.clone()).await?;
                        }
                    } else if *state == ElementState::Released {
                        if self.core.read().await.is_pressed() {
                            self.core
                                .write()
                                .await
                                .release(*in_slot, source.clone())
                                .await?;
                        }
                    }
                }
            }
            _ => {}
        };
        Ok(())
    }
}

impl Panel for Button {
    fn outer_frame(&self) -> Visual {
        self.container.clone().into()
    }
    fn id(&self) -> usize {
        Arc::as_ptr(&self.id) as usize
    }
}

pub trait ButtonSkin: Panel + EventSink<ButtonEvent, Error = crate::Error> {}
impl<T: Panel + EventSink<ButtonEvent, Error = crate::Error>> ButtonSkin for T {}

#[derive(EventSink)]
#[event_sink(event=PanelEvent)]
#[event_sink(event=ButtonEvent)]
pub struct SimpleButtonSkin {
    layer_stack: LayerStack,
    text: Arc<Text>,
    background: Arc<Background>,
    panel_events: EventStreams<PanelEvent>,
}

#[derive(TypedBuilder)]
pub struct SimpleButtonSkinParams<T: Spawn> {
    compositor: Compositor,
    text: String,
    color: Color,
    spawner: T,
}

impl<T: Spawn> TryFrom<SimpleButtonSkinParams<T>> for SimpleButtonSkin {
    type Error = crate::Error;
    fn try_from(value: SimpleButtonSkinParams<T>) -> crate::Result<Self> {
        let background: Arc<Background> = BackgroundParams::builder()
            .color(value.color)
            .round_corners(true)
            .compositor(value.compositor.clone())
            .build()
            .try_into()?;
        let text: Arc<Text> = TextParams::builder()
            .compositor(value.compositor.clone())
            .text(value.text)
            .spawner(value.spawner)
            .build()
            .try_into()?;
        let layer_stack = LayerStackParams::builder()
            .compositor(value.compositor.clone())
            .build()
            .push_panel(background.clone())
            .push_panel(text.clone())
            .try_into()?;
        Ok(SimpleButtonSkin {
            layer_stack,
            background,
            text,
            panel_events: EventStreams::new(),
        })
    }
}

impl<T: Spawn> TryFrom<SimpleButtonSkinParams<T>> for Arc<SimpleButtonSkin> {
    type Error = crate::Error;

    fn try_from(value: SimpleButtonSkinParams<T>) -> crate::Result<Self> {
        Ok(Arc::new(value.try_into()?))
    }
}

#[async_trait]
impl EventSinkExt<ButtonEvent> for SimpleButtonSkin {
    type Error = crate::Error;
    async fn on_event<'a>(
        &'a self,
        event: Cow<'a, ButtonEvent>,
        _: Option<Arc<EventBox>>,
    ) -> crate::Result<()> {
        match event.as_ref() {
            ButtonEvent::Press => self.background.set_color(Colors::DarkMagenta()?).await?,
            ButtonEvent::Release(_) => self.background.set_color(Colors::Magenta()?).await?,
        }
        Ok(())
    }
}

#[async_trait]
impl EventSinkExt<PanelEvent> for SimpleButtonSkin {
    type Error = crate::Error;
    async fn on_event<'a>(
        &'a self,
        event: Cow<'a, PanelEvent>,
        source: Option<Arc<EventBox>>,
    ) -> crate::Result<()> {
        self.layer_stack.on_event(event, source).await
    }
}

impl EventSource<PanelEvent> for SimpleButtonSkin {
    fn event_stream(&self) -> EventStream<PanelEvent> {
        self.panel_events.create_event_stream()
    }
}

impl Panel for SimpleButtonSkin {
    fn outer_frame(&self) -> Visual {
        self.layer_stack.outer_frame()
    }
    fn id(&self) -> usize {
        Arc::as_ptr(&self.text) as usize
    }
}
