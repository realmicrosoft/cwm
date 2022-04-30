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

use std::ffi::CStr;
use std::os::raw::c_int;
use std::ptr;
use std::ptr::null;
use libsex::bindings::*;
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

pub fn draw_x_window(conn: &Connection, window: CumWindow, display: *mut Display, visual: *mut XVisualInfo, fbconfigs: *mut GLXFBConfig) {
    // get pixmap of window
    let pixmap_id = conn.generate_id();
    let cookie = conn.send_request(&xcb::x::CreatePixmap {
        depth: 24,
        pid: pixmap_id,
        drawable: xcb::x::Drawable::Window(window.window_id),
        width: window.width,
        height: window.height,
    });

    conn.flush();

    // now unsafe time!
    unsafe {

        let glx_pixmap = glXCreatePixmap(display, *fbconfigs.offset(0),
                                         pixmap_id.resource_id() as u64, null());
        let mut texture: GLuint = 0;
        glGenTextures(1, &mut texture);
        glBindTexture(GL_TEXTURE_2D, texture);

        glXBindTexImageEXT(display, glx_pixmap, GLX_FRONT_LEFT_EXT as c_int, ptr::null_mut());

        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as GLint);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as GLint);

        let mut top: GLfloat;
        let bottom: GLfloat;

        glBegin(GL_QUADS);
        glTexCoord2d(0.0, 0.0);
        glVertex2d(0.0, 0.0);
        glTexCoord2d(1.0, 0.0);
        glVertex2d(window.width as GLdouble, 0.0);
        glTexCoord2d(1.0, 1.0);
        glVertex2d(window.width as GLdouble, window.height as GLdouble);
        glTexCoord2d(0.0, 1.0);
        glVertex2d(0.0, window.height as GLdouble);
        glEnd();

        glXReleaseTexImageEXT(display, glx_pixmap, GLX_FRONT_LEFT_EXT as c_int);
    }
}