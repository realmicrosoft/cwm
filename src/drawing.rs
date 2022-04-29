#![allow(clippy::too_many_arguments)]

use std::sync::Mutex;

use slog::Logger;

use smithay::{
    backend::renderer::{Frame, ImportAll, Renderer, Texture},
    desktop::space::{RenderElement, SpaceOutputTuple, SurfaceTree},
    reexports::wayland_server::protocol::wl_surface,
    utils::{Logical, Point, Rectangle, Size, Transform},
    wayland::{
        compositor::{get_role, with_states},
        seat::CursorImageAttributes,
    },
};

pub static CLEAR_COLOR: [f32; 4] = [0.8, 0.8, 0.9, 1.0];

smithay::custom_elements! {
    pub CustomElem<R>;
    SurfaceTree=SurfaceTree,
    PointerElement=PointerElement::<<R as Renderer>::TextureId>,
}

pub fn draw_cursor(
    surface: wl_surface::WlSurface,
    location: impl Into<Point<i32, Logical>>,
) -> SurfaceTree {
    let mut position = location.into();
    let ret = with_states(&surface, |states| {
        Some(
            states
                .data_map
                .get::<Mutex<CursorImageAttributes>>()
                .unwrap()
                .lock()
                .unwrap()
                .hotspot,
        )
    })
        .unwrap_or(None);
    position -= match ret {
        Some(h) => h,
        None => {
            println!(
                "Trying to display as a cursor a surface that does not have the CursorImage role."
            );
            (0, 0).into()
        }
    };
    SurfaceTree {
        surface,
        position,
        z_index: 100, /* Cursor should always be on-top */
    }
}

pub struct PointerElement<T: Texture> {
    texture: T,
    position: Point<i32, Logical>,
    size: Size<i32, Logical>,
}

impl<T: Texture> PointerElement<T> {
    pub fn new(texture: T, pointer_pos: Point<i32, Logical>) -> PointerElement<T> {
        let size = texture.size().to_logical(1, Transform::Normal);
        PointerElement {
            texture,
            position: pointer_pos,
            size,
        }
    }
}

impl<R> RenderElement<R> for PointerElement<<R as Renderer>::TextureId>
    where
        R: Renderer + ImportAll,
        <R as Renderer>::TextureId: 'static,
{
    fn id(&self) -> usize {
        0
    }

    fn geometry(&self) -> Rectangle<i32, Logical> {
        Rectangle::from_loc_and_size(self.position, self.size)
    }

    fn accumulated_damage(&self, _: Option<SpaceOutputTuple<'_, '_>>) -> Vec<Rectangle<i32, Logical>> {
        vec![Rectangle::from_loc_and_size((0, 0), self.size)]
    }

    fn draw(
        &self,
        _renderer: &mut R,
        frame: &mut <R as Renderer>::Frame,
        scale: f64,
        location: Point<i32, Logical>,
        damage: &[Rectangle<i32, Logical>],
        _log: &slog::Logger,
    ) -> Result<(), <R as Renderer>::Error> {
        frame.render_texture_at(
            &self.texture,
            location.to_f64().to_physical(scale).to_i32_round(),
            1,
            scale as f64,
            Transform::Normal,
            &*damage
                .iter()
                .map(|rect| rect.to_f64().to_physical(scale).to_i32_round())
                .collect::<Vec<_>>(),
            1.0,
        )?;
        Ok(())
    }
}