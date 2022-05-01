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

use std::ffi::{c_void, CStr};
use std::os::raw::{c_int, c_uint, c_ulong};
use std::{mem, ptr};
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

pub unsafe fn get_window_fb_config(window: Window, display: *mut Display, screen: *mut Screen) -> GLXFBConfig {
    let mut attrib = XWindowAttributes {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        border_width: 0,
        depth: 0,
        visual: null_mut(),
        root: 0,
        class: 0,
        bit_gravity: 0,
        win_gravity: 0,
        backing_store: 0,
        backing_planes: 0,
        backing_pixel: 0,
        save_under: 0,
        colormap: 0,
        map_installed: 0,
        map_state: 0,
        all_event_masks: 0,
        your_event_mask: 0,
        do_not_propagate_mask: 0,
        override_redirect: 0,
        screen
    };
    XGetWindowAttributes(display, window, &mut attrib);
    let visualid = XVisualIDFromVisual(attrib.visual);
    let mut visinfo: *mut XVisualInfo = null_mut();
    let mut wanted_config = 0;
    let mut value: c_int = 0;
    let mut nfbconfigs = 10;
    let fbconfigs = glXGetFBConfigs(display, XScreenNumberOfScreen(screen), &mut nfbconfigs);
    if fbconfigs.is_null() {
        panic!("Could not get fbconfigs");
    }
    for i in 0..nfbconfigs {
        visinfo = glXGetVisualFromFBConfig (display, *fbconfigs.offset(i as isize));
        if visinfo.is_null() || (*visinfo).visualid != visualid as u64 {
            continue;
        }

        // check if fbconfig supports drawing
        glXGetFBConfigAttrib(display, *fbconfigs.offset(i as isize), GLX_DRAWABLE_TYPE as c_int, &mut value);
        if value & GLX_PIXMAP_BIT as c_int == 0 {
            continue;
        }

        // check if fbconfig supports GLX_BIND_TO_TEXTURE_TARGETS_EXT
        glXGetFBConfigAttrib(display, *fbconfigs.offset(i as isize), GLX_BIND_TO_TEXTURE_TARGETS_EXT as c_int, &mut value);
        if value & GLX_TEXTURE_2D_BIT_EXT as c_int == 0 {
            continue;
        }

        // check if fbconfig supports GLX_BIND_TO_TEXTURE_RGBA_EXT
        glXGetFBConfigAttrib(display, *fbconfigs.offset(i as isize), GLX_BIND_TO_TEXTURE_RGBA_EXT as c_int, &mut value);
        if value & GLX_RGBA_BIT as c_int == 0 {
            // check if fbconfig supports GLX_BIND_TO_TEXTURE_RGB_EXT
            glXGetFBConfigAttrib(display, *fbconfigs.offset(i as isize), GLX_BIND_TO_TEXTURE_RGB_EXT as c_int, &mut value);
            if value & GLX_RGBA_BIT as c_int == 0 {
                continue;
            }
        }

        wanted_config = i;
        break;
    }

    *fbconfigs.offset(wanted_config as isize)
}

pub fn draw_x_window(window: CumWindow, display: *mut Display, visual: *mut XVisualInfo, value: c_int) {
    // now unsafe time!
    unsafe {
        let window_id = window.window_id;

        let pixmap = XCompositeNameWindowPixmap(display, window_id);
        XSync(display, 0);

        let mut texture: GLuint = 0;
        glEnable(GL_TEXTURE_2D);
        glGenTextures(1, &mut texture);
        glBindTexture(GL_TEXTURE_2D, texture);

        glXBindTexImageEXT(display, pixmap, GLX_FRONT_LEFT_EXT as c_int, null_mut());

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
        glTexEnvf(GL_TEXTURE_ENV, GL_TEXTURE_ENV_MODE, GL_MODULATE as GLfloat);



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

        glXReleaseTexImageEXT(display, pixmap, GLX_FRONT_LEFT_EXT as c_int);
        glDeleteTextures(1, &texture);
        XFreePixmap(display, pixmap);
    }
}