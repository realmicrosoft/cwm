mod types;
mod helpers;

use std::os::raw::{c_ulong};
use std::time::SystemTime;
use resize::Pixel::RGBA8;
use resize::px::RGBA;
use resize::Type::Lanczos3;
use stb_image::image::LoadResult;
use xcb::{composite, glx, x, Xid};
use crate::types::CumWindow;
use crate::helpers::rgba_to_bgra;

/*
unsafe extern "C" fn error_handler(display: *mut Display, error_event: *mut XErrorEvent) -> c_int {
    unsafe { println!("X Error: {}", (*error_event).error_code); }
    0
}
 */

fn main() {
    let (conn, screen_num) = xcb::Connection::connect(None).expect("Failed to connect to X server");
    let setup = conn.get_setup();
    let screen = setup.roots().nth(screen_num as usize).unwrap();
    // get root window
    let root = screen.root();

    // get dimensions
    let mut src_width = screen.width_in_pixels();
    let mut src_height = screen.height_in_pixels();

    let mut windows: Vec<types::CumWindow> = Vec::new();

    let mut accent_color = 0xFFFF0000;

    // get root window and set attributes so we receive events
    // gotta enable events in the
    let cookie = conn.send_request_checked(&x::ChangeWindowAttributes {
        window: root,
        value_list: &[
            x::Cw::EventMask(
                x::EventMask::BUTTON_PRESS |
                x::EventMask::BUTTON_RELEASE |
                x::EventMask::KEY_PRESS |
                x::EventMask::KEY_RELEASE |
                x::EventMask::EXPOSURE |
                x::EventMask::SUBSTRUCTURE_NOTIFY |
                //x::EventMask::SUBSTRUCTURE_REDIRECT |
                x::EventMask::STRUCTURE_NOTIFY
            )
        ],});

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error setting event mask, is another window manager running?");
    }

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
    let cookie = conn.send_request(&xcb::bigreq::Enable{});

    let reply = conn.wait_for_reply(cookie);
    if reply.is_err() {
        println!("Error enabling bigreq extension");
    }

    // check maximum request size
    println!("Maximum request size: {}", reply.unwrap().maximum_request_length());

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
        visual: screen.root_visual(),
        value_list: &[
            x::Cw::EventMask(x::EventMask::EXPOSURE),
        ],
    });

    conn.flush().expect("Could not flush");

    let gcon_id: x::Gcontext = conn.generate_id();
    // create a graphics context
    let cookie = conn.send_request_checked(&xcb::x::CreateGc {
        cid: gcon_id,
        drawable: x::Drawable::Window(desktop_id),
        value_list: &[
            x::Gc::Foreground(accent_color),
            x::Gc::GraphicsExposures(true)
        ],
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error creating graphics context");
    }

    // create glx context
    let glx_id = conn.generate_id();
    let cookie = conn.send_request_checked(&glx::CreateContext {
        context: glx_id,
        visual: screen.root_visual(),
        screen: screen_num as u32,
        share_list: glx::Context::none(),
        is_direct: true,
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error creating glx context");
    }

    // load the bg.png image
    let bg_image_load = stb_image::image::load("bg.png");
    let bg_image = match bg_image_load {
        LoadResult::ImageU8(image) => image,
        LoadResult::ImageF32(_) => panic!("bg.png is not 8-bit"),
        LoadResult::Error(e) => panic!("Error loading bg.png: {}", e),
    };

    let mut src = vec![RGBA::new(0,0,0,0); bg_image.width as usize * bg_image.height as usize];

    // copy the image data into the src vector
    let mut i = 0;
    while i < bg_image.data.len() {
        let r = bg_image.data[i];
        let g = bg_image.data[i + 1];
        let b = bg_image.data[i + 2];
        let a = bg_image.data[i + 3];
        src[i / 4] = RGBA::new(r, g, b, a);
        i += 4;
    }

    let mut dst = vec![RGBA::new(0,0,0,0); src_width as usize * src_height as usize];
    let mut resizer = resize::new(bg_image.width, bg_image.height, src_width as usize, src_height as usize, RGBA8, Lanczos3);
    // is resizer ok?
    if resizer.is_ok() {
        resizer.expect("resizer failed to unwrap").resize(&src, &mut dst);
    } else {
        println!("Error resizing bg.png");
    }

    let mut bg_image_data = vec![0; dst.len() * 4];
    let mut i = 0;
    while i < dst.len() {
        bg_image_data.push(dst[i].r);
        bg_image_data.push(dst[i].g);
        bg_image_data.push(dst[i].b);
        bg_image_data.push(255);
        i += 4;
    }

    // create a pixmap to draw on
    let bg_id = conn.generate_id();
    let cookie = conn.send_request_checked(&xcb::x::CreatePixmap {
        depth: 24,
        pid: bg_id,
        drawable: x::Drawable::Window(desktop_id),
        width: src_width as u16,
        height: src_height as u16,
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error creating pixmap");
        println!("{:?}", checked);
    }

    // put the image on the pixmap
    let cookie = conn.send_request_checked(&xcb::x::PutImage {
        drawable: x::Drawable::Pixmap(bg_id),
        gc: gcon_id,
        width: bg_image.width as u16,
        height: bg_image.height as u16,
        dst_x: 0,
        dst_y: 0,
        left_pad: 0,
        depth: 24,
        format: x::ImageFormat::ZPixmap,
        data: &rgba_to_bgra(&bg_image_data),
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error putting image on pixmap");
    }

    // create picture from pixmap
    let pic_id = conn.generate_id();
    let cookie = conn.send_request_checked(&xcb::render::CreatePicture {
        pid: pic_id,
        drawable: x::Drawable::Pixmap(bg_id),
        format: pict_format,
        value_list: &[
        ],
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error creating picture");
    }

    // get picture of window
    let desktop_pic_id = conn.generate_id();
    let cookie = conn.send_request_checked(&xcb::render::CreatePicture {
        pid: desktop_pic_id,
        drawable: x::Drawable::Window(desktop_id),
        format: pict_format,
        value_list: &[
        ],
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error creating picture");
    }

    // composite picture onto window
    let cookie = conn.send_request_checked(&xcb::render::Composite {
        op: xcb::render::PictOp::Over,
        src: pic_id,
        mask: xcb::render::Picture::none(),
        dst: desktop_pic_id,
        src_x: 0,
        src_y: 0,
        mask_x: 0,
        mask_y: 0,
        dst_x: 0,
        dst_y: 0,
        width: src_width,
        height: src_height,
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error compositing picture");
    }

    // map the window
    let cookie = conn.send_request_checked(&xcb::x::MapWindow {
        window: desktop_id,
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error mapping window");
    }

    conn.flush().expect("flush failed!");

    let mut now = SystemTime::now();
    let mut t = 0;

    loop {
        let event_pending = conn.poll_for_event();
        // if we have an event
        if let Ok(event_success) = event_pending {
            if event_success.is_some() {
                match event_success.unwrap() {
                    xcb::Event::X(x::Event::CreateNotify(ev)) => {
                        // set border width
                        conn.send_request(&x::ConfigureWindow {
                            window: ev.window(),
                            value_list: &[x::ConfigWindow::BorderWidth(5)],
                        });

                        conn.flush().expect("flush failed!");

                        // check the parent window to see if it's the root window
                        if root != ev.parent() {
                            continue;
                        }

                        // get pixmap for window
                        let p_id = conn.generate_id();
                        conn.send_request(&xcb::render::CreatePicture{
                            pid: p_id,
                            drawable: x::Drawable::Window(ev.window()),
                            format: pict_format,
                            value_list: &[
                                xcb::render::Cp::SubwindowMode(xcb::x::SubwindowMode::IncludeInferiors),
                            ],
                        });

                        // create copy of window bounding region
                        let r_id = conn.generate_id();
                        conn.send_request(&xcb::xfixes::CreateRegionFromWindow {
                            region: r_id,
                            window: ev.window(),
                            kind: xcb::shape::Sk::Bounding,
                        });

                        // translate it
                        conn.send_request(&xcb::xfixes::TranslateRegion {
                            region: r_id,
                            dx: -ev.x(),
                            dy: -ev.y(),
                        });
                        conn.send_request(&xcb::xfixes::SetPictureClipRegion {
                            picture: p_id,
                            region: r_id,
                            x_origin: 0,
                            y_origin: 0,
                        });

                        windows.push(CumWindow {
                            window_id: ev.window(),
                            pixmap_id: p_id,
                            region_id: r_id,
                            x: ev.x(),
                            y: ev.y(),
                            width: ev.width(),
                            height: ev.height(),
                            is_opening: false,
                            animation_time: 0
                        });
                    },
                    xcb::Event::X(x::Event::DestroyNotify(ev)) => {
                        windows.retain(|w| w.window_id != ev.window());
                    },
                    xcb::Event::X(x::Event::ConfigureNotify(ev)) => {
                        // check if the window is the root window
                        if ev.window() == root {
                            src_height = ev.height();
                            src_width = ev.width();
                        }
                        let mut found = false;
                        for w in windows.iter_mut() {
                            if w.window_id == ev.window() {
                                found = true;

                                // update bounding region
                                conn.send_request(&xcb::xfixes::CreateRegionFromWindow {
                                    region: w.region_id,
                                    window: ev.window(),
                                    kind: xcb::shape::Sk::Bounding,
                                });

                                // translate it
                                conn.send_request(&xcb::xfixes::TranslateRegion {
                                    region: w.region_id,
                                    dx: -ev.x(),
                                    dy: -ev.y(),
                                });
                                conn.send_request(&xcb::xfixes::SetPictureClipRegion {
                                    picture: w.pixmap_id,
                                    region: w.region_id,
                                    x_origin: 0,
                                    y_origin: 0,
                                });

                                w.x = ev.x();
                                w.y = ev.y();
                                w.width = ev.width();
                                w.height = ev.height();
                                break;
                            }
                        }
                        if !found {
                            // window not found, ignore
                            continue;
                        }
                    },
                    xcb::Event::X(x::Event::Expose(ev)) => {
                        // map window
                        conn.send_request(&x::MapWindow {
                            window: ev.window(),
                        });

                        // if desktop window, copy pixmap to window
                        if ev.window() == desktop_id {
                            let cookie = conn.send_request_checked(&xcb::render::Composite {
                                op: xcb::render::PictOp::Over,
                                src: pic_id,
                                mask: xcb::render::Picture::none(),
                                dst: desktop_pic_id,
                                src_x: 0,
                                src_y: 0,
                                mask_x: 0,
                                mask_y: 0,
                                dst_x: 0,
                                dst_y: 0,
                                width: src_width,
                                height: src_height,
                            });

                            let checked = conn.check_request(cookie);
                            if checked.is_err() {
                                println!("Error compositing picture");
                            }
                        }
                        conn.flush().expect("Error flushing");
                    },
                    _ => {}
                }
            }

            let after = SystemTime::now();
            if after.duration_since(now).unwrap().as_millis() > 10 {
                // generate the rainbow using a sine wave
                let frequency = 0.05;
                let r = ((frequency * (t as f64) + 0.0).sin() * 127.0f64 + 128.0f64) as c_ulong;
                let g = ((frequency * (t as f64) + 2.0).sin() * 127.0f64 + 128.0f64) as c_ulong;
                let b = ((frequency * (t as f64) + 4.0).sin() * 127.0f64 + 128.0f64) as c_ulong;

                accent_color = (((r << 16) | (g << 8) | (b)) | 0xFF000000) as u32;
                t += 1;

                // get root pixmap
                let r_id = conn.generate_id();
                conn.send_request(&xcb::render::CreatePicture{
                    pid: r_id,
                    drawable: x::Drawable::Window(root),
                    format: pict_format,
                    value_list: &[
                        xcb::render::Cp::SubwindowMode(xcb::x::SubwindowMode::IncludeInferiors),
                    ],
                });

                // for each window, move it by 1 up and 1 right
                for w in windows.iter_mut() {
                    // change the window's position
                    conn.send_request(&x::ConfigureWindow {
                        window: w.window_id,
                        value_list: &[
                            x::ConfigWindow::X(w.x as i32),
                            x::ConfigWindow::Y(w.y as i32),
                    ]});
                    // set the window's border color
                    conn.send_request(&x::ChangeWindowAttributes {
                        window: w.window_id,
                        value_list: &[
                            x::Cw::BorderPixel(accent_color as u32),
                    ]});

                    conn.flush().expect("Error flushing");

                    // get window pixmap
                    let p_id = conn.generate_id();
                    conn.send_request(&xcb::render::CreatePicture{
                        pid: p_id,
                        drawable: x::Drawable::Window(w.window_id),
                        format: pict_format,
                        value_list: &[
                            xcb::render::Cp::SubwindowMode(xcb::x::SubwindowMode::IncludeInferiors),
                        ],
                    });

                    if desktop_id != w.window_id {
                        w.x += 1;
                        w.y += 1;
                        if w.x >= src_width as i16 {
                            w.x = 0 - w.width as i16;
                        }
                        if w.y >= src_height as i16 {
                            w.y = 0 - w.height as i16;
                        }
                    }

                    // composite render pixmap onto window
                    conn.send_request(&xcb::render::Composite {
                        op: xcb::render::PictOp::Over,
                        src: p_id,
                        mask: xcb::render::Picture::none(),
                        dst: r_id,
                        src_x: -5,
                        src_y: -5,
                        mask_x: 0,
                        mask_y: 0,
                        dst_x: w.x - 5,
                        dst_y: w.y - 5,
                        width: w.width as u16 + 10,
                        height: w.height as u16 + 10,
                    });
                }
                conn.flush().expect("Error flushing");
                now = after;
            }
        }
    }
}
