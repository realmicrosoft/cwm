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

use xcb::Connection;
use xcb::{Xid};

pub fn allow_input_passthrough(&conn: &Connection, window: xcb::x::Window, p_id: xcb::render::Picture,x: i16, y: i16) -> xcb::xfixes::Region {
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

pub fn glx_pixmap_from_x(conn: &Connection, x_pixmap: xcb::x::Pixmap, screen_num: u32) -> xcb::glx::Pixmap {
    let glx_pixmap = conn.generate_id();
    conn.send_request(&xcb::glx::CreatePixmap {
        screen: screen_num,
        fbconfig: xcb::glx::Fbconfig::none(),
        pixmap: x_pixmap,
        glx_pixmap,
        num_attribs: 0,
        attribs: &[],
    });
    glx_pixmap
}