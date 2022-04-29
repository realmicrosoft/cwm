use std::cell::RefCell;
use std::ptr::null_mut;
use slog::Logger;
use smithay::{
    backend::renderer::{Frame, ImportAll, Renderer},
    desktop::{
        draw_window,
        space::{RenderElement, RenderError, Space},
    },
    utils::{Logical, Rectangle},
    wayland::output::Output,
};
use smithay::desktop::Window;

#[derive(Default)]
pub struct FullscreenSurface(RefCell<Option<Window>>);

impl FullscreenSurface {
    pub fn set(&self, window: Window) {
        *self.0.borrow_mut() = Some(window);
    }

    pub fn get(&self) -> Option<Window> {
        self.0.borrow().clone()
    }

    pub fn clear(&self) -> Option<Window> {
        self.0.borrow_mut().take()
    }
}

use crate::{drawing::*};

pub fn render_output<R, E>(
    output: &Output,
    space: &mut Space,
    renderer: &mut R,
    age: usize,
    elements: &[E],
) -> Result<Option<Vec<Rectangle<i32, Logical>>>, RenderError<R>>
    where
        R: Renderer + ImportAll,
        R::TextureId: 'static,
        E: RenderElement<R>,
{
    if let Some(window) = output
        .user_data()
        .get::<FullscreenSurface>()
        .and_then(|f| f.get())
    {
        let transform = output.current_transform().into();
        let mode = output.current_mode().unwrap();
        let scale = output.current_scale().fractional_scale();
        let output_geo = space
            .output_geometry(output)
            .unwrap_or_else(|| Rectangle::from_loc_and_size((0, 0), (0, 0)));
        renderer
            .render(mode.size, transform, |renderer, frame| {
                let mut damage = window.accumulated_damage(None);
                frame.clear(
                    CLEAR_COLOR,
                    &[Rectangle::from_loc_and_size((0, 0), mode.size).to_f64()],
                )?;
                let dummy_logger = slog::Logger::root(slog::Discard, slog::o!());
                draw_window(
                    renderer,
                    frame,
                    &window,
                    scale,
                    (0, 0),
                    &[Rectangle::from_loc_and_size(
                        (0, 0),
                        mode.size.to_f64().to_logical(scale).to_i32_round(),
                    )],
                    &dummy_logger,
                )?;
                for elem in elements {
                    let geo = elem.geometry();
                    let location = geo.loc - output_geo.loc;
                    let elem_damage = elem.accumulated_damage(None);
                    elem.draw(
                        renderer,
                        frame,
                        scale,
                        location,
                        &[Rectangle::from_loc_and_size((0, 0), geo.size)],
                        &dummy_logger,
                    )?;
                    damage.extend(elem_damage.into_iter().map(|mut rect| {
                        rect.loc += geo.loc;
                        rect
                    }))
                }
                Ok(Some(damage))
            })
            .and_then(std::convert::identity)
            .map_err(RenderError::<R>::Rendering)
    } else {
        space.render_output(&mut *renderer, output, age as usize, CLEAR_COLOR, &*elements)
    }
}