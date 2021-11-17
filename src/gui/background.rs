use async_object::{Keeper, Tag};
use float_ord::FloatOrd;
use futures::{
    task::{Spawn, SpawnExt},
    StreamExt,
};
use windows::{
    Foundation::Numerics::Vector2,
    UI::{
        Color,
        Composition::{CompositionShape, Compositor, ShapeVisual},
    },
};

use crate::gui::{SlotPlug, SlotTag};
use crate::unwrap_err;

pub struct Background {
    compositor: Compositor,
    slot: SlotPlug,
    shape: ShapeVisual,
    round_corners: bool,
    color: Color,
}

impl Background {
    fn new(
        compositor: &Compositor,
        slot: SlotTag,
        color: Color,
        round_corners: bool,
    ) -> crate::Result<Self> {
        let compositor = compositor.clone();
        let shape = compositor.CreateShapeVisual()?;
        let slot = slot.plug(shape.clone().into())?;
        let background = Self {
            compositor,
            slot,
            shape,
            color,
            round_corners,
        };
        background.redraw()?;
        Ok(background)
    }

    fn set_color(&mut self, color: Color) -> crate::Result<()> {
        self.color = color;
        self.redraw()?;
        Ok(())
    }

    fn set_size(&mut self, size: Vector2) -> crate::Result<()> {
        self.shape.SetSize(size)?;
        self.redraw()?;
        Ok(())
    }

    fn redraw(&self) -> crate::Result<()> {
        self.shape.Shapes()?.Clear()?;
        self.shape
            .Shapes()?
            .Append(self.create_background_shape()?)?;
        Ok(())
    }
    fn create_background_shape(&self) -> crate::Result<CompositionShape> {
        let container_shape = self.compositor.CreateContainerShape()?;
        let rect_geometry = self.compositor.CreateRoundedRectangleGeometry()?;
        rect_geometry.SetSize(self.shape.Size()?)?;
        if self.round_corners {
            let size = rect_geometry.Size()?;
            let radius = std::cmp::min(FloatOrd(size.X), FloatOrd(size.Y)).0 / 20.;
            rect_geometry.SetCornerRadius(Vector2 {
                X: radius,
                Y: radius,
            })?;
        } else {
            rect_geometry.SetCornerRadius(Vector2 { X: 0., Y: 0. })?;
        }
        let brush = self
            .compositor
            .CreateColorBrushWithColor(self.color.clone())?;
        let rect = self
            .compositor
            .CreateSpriteShapeWithGeometry(rect_geometry)?;
        rect.SetFillBrush(brush)?;
        rect.SetOffset(Vector2 { X: 0., Y: 0. })?;
        container_shape.Shapes()?.Append(rect)?;
        let shape = container_shape.into();
        Ok(shape)
    }
}

pub struct BackgroundKeeper(Keeper<Background>);

impl BackgroundKeeper {
    pub fn new(
        spawner: impl Spawn,
        compositor: &Compositor,
        slot: SlotTag,
        color: Color,
        round_corners: bool,
    ) -> crate::Result<Self> {
        let keeper = Keeper::new(Background::new(compositor, slot, color, round_corners)?);
        let keeper = Self(keeper);
        keeper.spawn_event_handlers(spawner)?;
        Ok(keeper)
    }
    pub fn tag(&self) -> BackgroundTag {
        BackgroundTag(self.0.tag())
    }
    fn spawn_event_handlers(&self, spawner: impl Spawn) -> crate::Result<()> {
        let tag = self.tag();
        let slot = self.0.get().slot.tag();
        let func = unwrap_err(async move {
            while let Some(size) = slot.on_slot_resize().next().await {
                tag.set_size(size.0).await?;
            }
            Ok(())
        });
        spawner.spawn(func)?;
        Ok(())
    }
}
#[derive(Clone, PartialEq)]
pub struct BackgroundTag(Tag<Background>);

impl BackgroundTag {
    pub async fn round_corners(&self) -> crate::Result<bool> {
        Ok(self.0.async_call(|v| v.round_corners).await?)
    }
    pub async fn color(&self) -> crate::Result<Color> {
        Ok(self.0.async_call(|v| v.color).await?)
    }
    pub async fn set_color(&self, color: Color) -> crate::Result<()> {
        Ok(self.0.async_call_mut(|v| v.set_color(color)).await??)
    }
    pub async fn set_size(&self, size: Vector2) -> crate::Result<()> {
        Ok(self.0.async_call_mut(|v| v.set_size(size)).await??)
    }
}