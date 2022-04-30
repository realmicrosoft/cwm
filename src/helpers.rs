pub fn rgba_to_bgra(rgba_array: &[u8]) -> Vec<u8> {
    let mut new_array = Vec::with_capacity(rgba_array.len());
    let mut i = 0;
    while i < rgba_array.len() {
        let r = rgba_array[i];
        let g = rgba_array[i + 1];
        let b = rgba_array[i + 2];
        let a = rgba_array[i + 3];
        new_array.push(b);
        new_array.push(g);
        new_array.push(r);
        new_array.push(a);
        i += 4;
    }
    new_array


}

use std::ffi::c_void;
use rogl::bitflags::{GL_COLOR_BUFFER_BIT, GL_DEPTH_BUFFER_BIT};
use rogl::command::PFN_glEnable;
use rogl::enums::{GL_ARRAY_BUFFER, GL_BGRA, GL_FALSE, GL_FLOAT, GL_LINEAR, GL_MODULATE, GL_QUADS, GL_RGB, GL_RGBA, GL_STATIC_DRAW, GL_TEXTURE_2D, GL_TEXTURE_ENV_MODE, GL_TEXTURE_MAG_FILTER, GL_TEXTURE_MIN_FILTER, GL_UNSIGNED_BYTE};
use rogl::gl::context::GLContext;
use rogl::gl::gl40::GL40;
use rogl::gl::gl33::GL33;
use sdl2::rect::Rect;
use xcb::Connection;
use xcb::{Xid};
use crate::CumWindow;

pub fn allow_input_passthrough(conn: &Connection, window: xcb::x::Window,x: i16, y: i16) -> xcb::xfixes::Region {
    // create copy of window bounding region
    let r_id = conn.generate_id();
    conn.send_request(&xcb::xfixes::CreateRegionFromWindow {
        region: r_id,
        window,
        kind: xcb::shape::Sk::Bounding,
    });
    // translate it
    conn.send_request(&xcb::xfixes::TranslateRegion {
        region: r_id,
        dx: -x,
        dy: -y,
    });
   /* conn.send_request(&xcb::xfixes::SetPictureClipRegion {
        picture: p_id,
        region: r_id,
        x_origin: 0,
        y_origin: 0,
    });
    */
    // delete the region
    conn.send_request(&xcb::xfixes::DestroyRegion {
        region: r_id,
    });

    r_id
}

pub fn draw_x_window(conn: &Connection, window: CumWindow, ctx: &sdl2::Sdl, mut canvas: &mut sdl2::render::WindowCanvas ) {
    let cookie = conn.send_request(&xcb::x::GetImage{
        format: xcb::x::ImageFormat::ZPixmap,
        drawable: xcb::x::Drawable::Window(window.window_id),
        x: 0,
        y: 0,
        width: window.width,
        height: window.height,
        plane_mask: 0xffffffff,
    });

    let reply = conn.wait_for_reply(cookie);
    let reply = reply.expect("Failed to get image");
    let image = reply.data();
    let mut image_vec: Vec<u8> = Vec::from(image);

    let surface = sdl2::surface::Surface::from_data(
        image_vec.as_mut_slice(),
        window.width as u32,
        window.height as u32,
        window.width as u32 * 4,
        sdl2::pixels::PixelFormatEnum::ARGB8888).unwrap();

    let texture_creator = canvas.texture_creator();
    let texture = texture_creator.create_texture_from_surface(&surface).unwrap();

    // draw
    canvas.copy(&texture, None, Rect::new(0, 0, window.width as u32, window.height as u32)).unwrap();

}

pub unsafe fn create_sdl2_context(src_width: u16, src_height: u16) -> (
    sdl2::Sdl,
    sdl2::video::Window,
    sdl2::EventPump,
) {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let gl_attr = video.gl_attr();
    let window = video
        .window("CHAOTIC WINDOW MANAGER", src_width as u32, src_height as u32)
        .position_centered()
        .build()
        .unwrap();
    let event_loop = sdl.event_pump().unwrap();

    (sdl, window, event_loop)
}