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
use std::ptr::{null, null_mut};
use libsex::bindings::*;
use crate::CumWindow;

pub fn allow_input_passthrough(display: *mut Display, win: Window, x: i16, y: i16) {
    unsafe {
        let region = XFixesCreateRegion(display, null_mut(), 0);
        XFixesSetWindowShapeRegion(display, win, ShapeBounding as c_int, 0, 0, 0);
        XFixesSetWindowShapeRegion(display, win, ShapeInput as c_int, 0, 0, region);
        XFixesDestroyRegion(display, region);
    }
}

pub fn draw_x_window(window: CumWindow, display: *mut Display, visual: *mut XVisualInfo, fbconfigs: GLXFBConfig, value: c_int) {
    // now unsafe time!
    unsafe {
        let pixmap = XCompositeNameWindowPixmap(display, window.window_id);

        let pixmap_attribs = [ GLX_TEXTURE_TARGET_EXT, GLX_TEXTURE_2D_EXT,
            GLX_TEXTURE_FORMAT_EXT, GLX_TEXTURE_FORMAT_RGBA_EXT,
            GLX_NONE ];

        let glx_pixmap = glXCreatePixmap(display, fbconfigs,
                                         pixmap, pixmap_attribs.as_ptr() as *const c_int);
        let mut texture: GLuint = 0;
        glGenTextures(1, &mut texture);
        glBindTexture(GL_TEXTURE_2D, texture);

        glXBindTexImageEXT(display, glx_pixmap, GLX_FRONT_LEFT_EXT as c_int, ptr::null_mut());

        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as GLint);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as GLint);

        let top: GLdouble;
        let bottom: GLdouble;


        if value == 1
        {
            top = 0.0;
            bottom = 1.0;
        }
        else
        {
            top = 1.0;
            bottom = 0.0;
        }

        glBegin(GL_QUADS);

        glTexCoord2d(0.0, bottom);
        glVertex2d(0.0, 0.0);

        glTexCoord2d(1.0, top);
        glVertex2d(0.0, 1.0);

        glTexCoord2d(1.0, top);
        glVertex2d(1.0, 1.0);

        glTexCoord2d(0.0, bottom);
        glVertex2d(1.0, 0.0);

        glEnd();

        glXReleaseTexImageEXT(display, glx_pixmap, GLX_FRONT_LEFT_EXT as c_int);
        glXDestroyPixmap(display, glx_pixmap);
        XFreePixmap(display, pixmap);
    }
}