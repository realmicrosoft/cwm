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
use xcb::Connection;
use xcb::{Xid};
use crate::CumWindow;

pub fn allow_input_passthrough(conn: &Connection, window: xcb::x::Window, p_id: xcb::render::Picture,x: i16, y: i16) -> xcb::xfixes::Region {
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
    conn.send_request(&xcb::xfixes::SetPictureClipRegion {
        picture: p_id,
        region: r_id,
        x_origin: 0,
        y_origin: 0,
    });
    // delete the region
    conn.send_request(&xcb::xfixes::DestroyRegion {
        region: r_id,
    });

    r_id
}

use rogl::gl::gl45::GL45;
use rogl::types::{GLboolean, GLfloat, GLint, GLsizei, GLsizeiptr, GLuint, GLvoid};

pub trait GL: GL45 {}
impl GL for GLContext {}

pub fn draw_x_window<T>(conn: &Connection, window: CumWindow, ctx: &T) where T: GL {
    unsafe {
        let mut texture: GLuint = 0;
        ctx.glEnable(GL_TEXTURE_2D);
        ctx.glGenTextures(GL_TEXTURE_2D as GLsizei, &mut texture);
        ctx.glBindTexture(GL_TEXTURE_2D, texture);
        ctx.glTexParameterf(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as GLfloat);
        ctx.glTexParameterf(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as GLfloat);
        ctx.glTexParameterf(GL_TEXTURE_2D, GL_TEXTURE_ENV_MODE, GL_MODULATE as GLfloat);

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
        let reply = reply.unwrap();
        let image = reply.data();
        let image_vec: Vec<u8> = Vec::from(image);


        ctx.glTexImage2D(GL_TEXTURE_2D, 0, GL_RGB as GLint,
                         window.width as GLsizei, window.height as GLsizei,
                         0, GL_BGRA, GL_UNSIGNED_BYTE,  image_vec.as_ptr() as *const c_void);

        ctx.glViewport(0, 0, window.width as GLsizei, window.height as GLsizei);
        ctx.glClearColor(0.3, 0.0, 0.3, 1.0);
        ctx.glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);

        let vertices: [GLfloat; 8] = [
            -1.0, -1.0,
            1.0, -1.0,
            1.0, 1.0,
            -1.0, 1.0,
        ];
        let mut vbo_buffers: [GLuint; 1] = [0];
        ctx.glCreateBuffers(1, &mut vbo_buffers[0]);
        ctx.glBindBuffer(GL_ARRAY_BUFFER, vbo_buffers[0]);
        ctx.glBufferData(GL_ARRAY_BUFFER,
                         8 * std::mem::size_of::<GLfloat>() as GLsizeiptr,
                         vertices.as_ptr() as *const c_void,
                         GL_STATIC_DRAW);

        let mut vao_buffers: [GLuint; 1] = [0];
        ctx.glCreateVertexArrays(1, &mut vao_buffers[0]);
        ctx.glBindVertexArray(vao_buffers[0]);
        ctx.glEnableVertexAttribArray(0);
        ctx.glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE as GLboolean, 0, std::ptr::null());

        // draw
        ctx.glDrawArrays(GL_QUADS, 0, 4);
        ctx.glDeleteVertexArrays(1, &mut vao_buffers[0]);
        ctx.glDeleteBuffers(1, &mut vbo_buffers[0]);
        ctx.glDeleteTextures(1, &mut texture);
    }
}

pub unsafe fn create_sdl2_context(src_width: u16, src_height: u16) -> (
    GLContext,
    sdl2::video::Window,
    sdl2::EventPump,
    sdl2::video::GLContext,
) {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let gl_attr = video.gl_attr();
    gl_attr.set_context_profile(sdl2::video::GLProfile::Core);
    gl_attr.set_context_version(3, 0);
    let window = video
        .window("CHAOTIC WINDOW MANAGER", src_width as u32, src_height as u32)
        .opengl()
        .build()
        .unwrap();
    let gl_context = window.gl_create_context().unwrap();
    let context = GLContext::load(|s| {
        video.gl_get_proc_address(s.to_str().expect("failed to conver string")) as *const _
    });
    let event_loop = sdl.event_pump().unwrap();

    (context, window, event_loop, gl_context)
}