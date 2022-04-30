use std::num::NonZeroU32;
use stb_image::image::LoadResult;
use xcb::{composite, Connection, x, Xid};
use crate::{allow_input_passthrough, fr, redraw_desktop, rgba_to_bgra};

pub fn setup_compositing(conn: &Connection, root: xcb::x::Window) -> (xcb::x::Window, xcb::render::Pictformat) {
    // query version of composite extension so that we get a panic early on if it's not available
    conn.send_request(&xcb::composite::QueryVersion {
        client_major_version: 1,
        client_minor_version: 0,
    });

    // redirect subwindows of root window
    let cookie = conn.send_request_checked(&xcb::composite::RedirectSubwindows {
        window: root,
        update: composite::Redirect::Manual,
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error redirecting subwindows, is another window manager running?");
    }

    // get pictformat
    let pict_format_cookie = conn.send_request(&xcb::render::QueryPictFormats {});
    let pict_format_reply = conn.wait_for_reply(pict_format_cookie);
    // go through all pictformats to find a suitable one
    let mut pict_format: xcb::render::Pictformat = xcb::render::Pictformat::none();
    for pict_format_reply in pict_format_reply.unwrap().formats() {
        if pict_format_reply.depth() == 24 {
            pict_format = pict_format_reply.id();
            break;
        }
    }

    // enable bigreq extension
    let cookie = conn.send_request(&xcb::bigreq::Enable {});

    let reply = conn.wait_for_reply(cookie);
    if reply.is_err() {
        println!("Error enabling bigreq extension");
    }
    // check maximum request size
    println!("Maximum request size: {}", reply.unwrap().maximum_request_length());

   /* // get overlay window
    let cookie = conn.send_request(&xcb::composite::GetOverlayWindow {
        window: root,
    });

    let reply = conn.wait_for_reply(cookie);
    if reply.is_err() {
        println!("Error getting overlay window");
    }

    let overlay_window = reply.unwrap().overlay_win();

    // get overlay picture
    let r_id = conn.generate_id();
    conn.send_request(&xcb::render::CreatePicture {
        pid: r_id,
        drawable: x::Drawable::Window(overlay_window),
        format: pict_format,
        value_list: &[
            xcb::render::Cp::SubwindowMode(xcb::x::SubwindowMode::IncludeInferiors),
        ],
    });

    // allow input passthrough
    allow_input_passthrough(&conn, overlay_window, 0, 0);

    */

    (xcb::x::Window::none(), pict_format)
}

pub fn setup_desktop(conn: &Connection, visual: xcb::x::Visualid,
                     pict_format:xcb::render::Pictformat,
                     g_context: x::Gcontext, root: xcb::x::Window,
                     src_width: u16, src_height: u16) -> (x::Window){
    // create new window for desktop
    let desktop_id = conn.generate_id();
    conn.send_request(&x::CreateWindow {
        depth: x::COPY_FROM_PARENT as u8,
        wid: desktop_id,
        parent: root,
        x: 0,
        y: 0,
        width: src_width,
        height: src_height,
        border_width: 0,
        class: x::WindowClass::InputOutput,
        visual,
        value_list: &[
            x::Cw::EventMask(x::EventMask::EXPOSURE),
        ],
    });

    conn.flush().expect("Could not flush");

    // load the bg.png image
    let bg_image_load = stb_image::image::load("bg.png");
    let bg_image = match bg_image_load {
        LoadResult::ImageU8(image) => image,
        LoadResult::ImageF32(_) => panic!("bg.png is not 8-bit"),
        LoadResult::Error(e) => panic!("Error loading bg.png: {}", e),
    };

    let bg_image_width = NonZeroU32::new(bg_image.width as u32).unwrap();
    let bg_image_height = NonZeroU32::new(bg_image.height as u32).unwrap();

    let mut divide_factor = 1;
    let mut potential_size: u32 = (src_width / divide_factor) as u32 * (src_height / divide_factor) as u32;
    // calculate the amount of bytes that src_width * src_height * 4 will take
    while potential_size > 300000 {
        divide_factor += 1;
        potential_size = (src_width / divide_factor) as u32 * (src_height / divide_factor) as u32;
    }

    let mut src = fr::Image::from_vec_u8(
        bg_image_width,
        bg_image_height,
        bg_image.data.clone(),
        fr::PixelType::U8x4,
    ).unwrap();
    // Create MulDiv instance
    let alpha_mul_div = fr::MulDiv::default();
    // Multiple RGB channels of source image by alpha channel
    alpha_mul_div
        .multiply_alpha_inplace(&mut src.view_mut())
        .unwrap();

    let dst_width = NonZeroU32::new((src_width / divide_factor) as u32).unwrap();
    let dst_height = NonZeroU32::new((src_height / divide_factor) as u32).unwrap();
    let mut dst_image = fr::Image::new(
        dst_width,
        dst_height,
        fr::PixelType::U8x4,
    );
    let mut dst_view = dst_image.view_mut();

    let mut resizer = fr::Resizer::new(
        fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3),
    );
    resizer.resize(&src.view(), &mut dst_view).unwrap();

    alpha_mul_div.divide_alpha_inplace(&mut dst_view).unwrap();

    let bg_image_data = dst_image.buffer();

    // create a pixmap to draw on
    let bg_id = conn.generate_id();
    let cookie = conn.send_request_checked(&xcb::x::CreatePixmap {
        depth: 24,
        pid: bg_id,
        drawable: x::Drawable::Window(desktop_id),
        width: (src_width / divide_factor) as u16,
        height: (src_height / divide_factor) as u16,
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error creating pixmap");
        println!("{:?}", checked);
    }

    // put the image on the pixmap
    let cookie = conn.send_request_checked(&xcb::x::PutImage {
        drawable: x::Drawable::Pixmap(bg_id),
        gc: g_context,
        width: (src_width / divide_factor) as u16,
        height: (src_height / divide_factor) as u16,
        dst_x: 0,
        dst_y: 0,
        left_pad: 0,
        depth: 24,
        format: x::ImageFormat::ZPixmap,
        data: &rgba_to_bgra(bg_image_data),
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error putting image on pixmap");
        println!("{:?}", checked);
    }

    // calculate the amount of times to multiply the image by
    let transform = [
        1, 0, 0,
        0, 1, 0,
        0, 0, divide_factor as i32,
    ];

    // create picture from pixmap
    let pic_id = conn.generate_id();
    let cookie = conn.send_request_checked(&xcb::render::CreatePicture {
        pid: pic_id,
        drawable: x::Drawable::Pixmap(bg_id),
        format: pict_format,
        value_list: &[],
    });
    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error creating picture");
    }

    // set picture transform
    let cookie = conn.send_request_checked(&xcb::render::SetPictureTransform {
        picture: pic_id,
        transform: xcb::render::Transform {
            matrix11: transform[0],
            matrix12: transform[1],
            matrix13: transform[2],
            matrix21: transform[3],
            matrix22: transform[4],
            matrix23: transform[5],
            matrix31: transform[6],
            matrix32: transform[7],
            matrix33: transform[8],
        },
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error setting picture transform");
    }

    // get picture of window
    let desktop_pic_id = conn.generate_id();
    let cookie = conn.send_request_checked(&xcb::render::CreatePicture {
        pid: desktop_pic_id,
        drawable: x::Drawable::Window(desktop_id),
        format: pict_format,
        value_list: &[],
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error creating picture");
    }

    // composite picture onto window
    redraw_desktop(&conn, pic_id, desktop_pic_id, src_width, src_height);

    // map the window
    let cookie = conn.send_request_checked(&xcb::x::MapWindow {
        window: desktop_id,
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error mapping window");
    }

    // flush all requests
    conn.flush();

    (desktop_id)
}