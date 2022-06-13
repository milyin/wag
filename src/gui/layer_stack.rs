use async_std::sync::{Arc, RwLock};

use super::{EventSink, EventSource, Panel, PanelEvent};
use async_events::{EventBox, EventQueues, EventStream};
use async_trait::async_trait;

use derive_weak::Weak;
use typed_builder::TypedBuilder;
use windows::UI::Composition::{Compositor, ContainerVisual};

struct Core {
    layers: Vec<Box<dyn Panel>>,
}

#[derive(Clone, Weak)]
pub struct LayerStack {
    container: ContainerVisual,
    core: Arc<RwLock<Core>>,
    events: Arc<EventQueues>,
}

impl LayerStack {
    async fn layers(&self) -> Vec<Box<dyn Panel>> {
        self.core.read().await.layers.clone()
    }

    pub async fn push_panel(&mut self, mut panel: impl Panel + 'static) -> crate::Result<()> {
        panel.attach(self.container.clone())?;
        self.core.write().await.layers.push(Box::new(panel));
        Ok(())
    }

    pub async fn remove_panel(&mut self, mut panel: impl Panel) -> crate::Result<()> {
        let mut core = self.core.write().await;
        if let Some(index) = core.layers.iter().position(|v| *v == panel) {
            panel.detach()?;
            core.layers.remove(index);
        }
        Ok(())
    }
    async fn translate_event_to_all_layers(
        &mut self,
        event: PanelEvent,
        source: Option<Arc<EventBox>>,
    ) -> crate::Result<()> {
        // TODO: run simultaneously
        for mut item in self.layers().await {
            item.on_event(event.clone(), source.clone()).await?;
        }
        Ok(())
    }
    async fn translate_event_to_top_layer(
        &mut self,
        event: PanelEvent,
        source: Option<Arc<EventBox>>,
    ) -> crate::Result<()> {
        if let Some(item) = self.layers().await.first_mut() {
            item.on_event(event, source).await?;
        }
        Ok(())
    }
    async fn translate_event(
        &mut self,
        event: PanelEvent,
        source: Option<Arc<EventBox>>,
    ) -> crate::Result<()> {
        match event {
            PanelEvent::Resized(size) => {
                self.container.SetSize(size)?;
                self.translate_event_to_all_layers(event, source).await
            }
            PanelEvent::MouseInput { .. } => self.translate_event_to_top_layer(event, source).await,
            _ => self.translate_event_to_all_layers(event, source).await,
        }
    }
}
// fn send_mouse_left_pressed(&mut self, event: MouseLeftPressed) -> crate::Result<()> {
//     for slot in &mut self.slots {
//         slot.send_mouse_left_pressed(event.clone())?;
//     }
//     Ok(())
// }

// fn send_mouse_left_pressed_focused(
//     &mut self,
//     event: MouseLeftPressedFocused,
// ) -> crate::Result<()> {
//     if let Some(slot) = self.slots.last_mut() {
//         slot.send_mouse_left_pressed_focused(event)?;
//     }
//     Ok(())
// }

#[derive(TypedBuilder)]
pub struct LayerStackParams {
    compositor: Compositor,
    #[builder(default)]
    layers: Vec<Box<dyn Panel>>,
}

impl LayerStackParams {
    pub fn push_panel(mut self, panel: impl Panel + 'static) -> Self {
        self.layers.push(Box::new(panel));
        self
    }
    pub fn create(self) -> crate::Result<LayerStack> {
        let mut layers = self.layers;
        let container = self.compositor.CreateContainerVisual()?;
        for layer in &mut layers {
            layer.attach(container.clone())?;
        }
        let core = Arc::new(RwLock::new(Core { layers }));
        // container.SetComment(HSTRING::from("LAYER_STACK"))?;
        Ok(LayerStack {
            container,
            core,
            events: Arc::new(EventQueues::new()),
        })
    }
}

impl Panel for LayerStack {
    fn id(&self) -> usize {
        Arc::as_ptr(&self.core) as usize
    }
    fn attach(&mut self, container: ContainerVisual) -> crate::Result<()> {
        container.Children()?.InsertAtTop(self.container.clone())?;
        Ok(())
    }
    fn detach(&mut self) -> crate::Result<()> {
        if let Ok(parent) = self.container.Parent() {
            parent.Children()?.Remove(&self.container.clone())?;
        }
        Ok(())
    }
    fn clone_panel(&self) -> Box<(dyn Panel + 'static)> {
        Box::new(self.clone())
    }
}

impl EventSource<PanelEvent> for LayerStack {
    fn event_stream(&self) -> EventStream<PanelEvent> {
        self.events.create_event_stream()
    }
}

#[async_trait]
impl EventSink<PanelEvent> for LayerStack {
    async fn on_event(
        &mut self,
        event: PanelEvent,
        source: Option<Arc<EventBox>>,
    ) -> crate::Result<()> {
        self.translate_event(event.clone(), source.clone()).await?;
        self.events.send_event(event, source).await;
        Ok(())
    }
}
