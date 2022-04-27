mod types;

use std::borrow::Borrow;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_ulong};
use std::ptr::{null, null_mut};
use std::time::SystemTime;
use xcb::{composite, glx, x, Xid, XidNew};
use crate::types::CumWindow;

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

    let pictformat: xcb::render::Pictformat;
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

    conn.flush();

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

                        conn.flush();

                        // check the parent window to see if it's the root window
                        if root != ev.parent() {
                            continue;
                        }

                        // get window attributes
                        let attr_cookie = conn.send_request(&x::GetWindowAttributes {
                            window: ev.window(),
                        });

                        let attr_reply = conn.wait_for_reply(attr_cookie);

                        // get pixmap for window
                        let p_id = conn.generate_id();
                        let w_pixmap = conn.send_request(&xcb::render::CreatePicture{
                            pid: p_id,
                            drawable: x::Drawable::Window(ev.window()),
                            format: pict_format,
                            value_list: &[
                                xcb::render::Cp::SubwindowMode(xcb::x::SubwindowMode::IncludeInferiors),
                            ],
                        });

                        // create copy of window bounding region
                        let r_id = conn.generate_id();
                        let w_region = conn.send_request(&xcb::xfixes::CreateRegionFromWindow {
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
                    },
                    _ => {}
                }
            }

            let after = SystemTime::now();
            if after.duration_since(now).unwrap().as_millis() > 10 {
                // generate the rainbow using a sine wave
                let frequency = 0.05;
                let mut r = ((frequency * (t as f64) + 0.0).sin() * 127.0f64 + 128.0f64) as c_ulong;
                let mut g = ((frequency * (t as f64) + 2.0).sin() * 127.0f64 + 128.0f64) as c_ulong;
                let mut b = ((frequency * (t as f64) + 4.0).sin() * 127.0f64 + 128.0f64) as c_ulong;

                accent_color = (((r << 16) | (g << 8) | (b)) | 0xFF000000);
                t += 1;

                // get root pixmap
                let r_id = conn.generate_id();
                let r_pixmap = conn.send_request(&xcb::render::CreatePicture{
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

                    conn.flush();

                    // get window pixmap
                    let p_id = conn.generate_id();
                    let w_pixmap = conn.send_request(&xcb::render::CreatePicture{
                        pid: p_id,
                        drawable: x::Drawable::Window(w.window_id),
                        format: pict_format,
                        value_list: &[
                            xcb::render::Cp::SubwindowMode(xcb::x::SubwindowMode::IncludeInferiors),
                        ],
                    });

                    // draw black rectangle so that we don't get the previous window
                    conn.send_request(&xcb::render::FillRectangles {
                        op: xcb::render::PictOp::Clear,
                        dst: r_id,
                        color: xcb::render::Color{red: 0, green: 0, blue: 0, alpha: 100},
                        rects: &[
                            xcb::x::Rectangle{
                                x: w.x as i16 - 5,
                                y: w.y as i16 - 5,
                                width: w.width as u16 + 10,
                                height: w.height as u16 + 10,
                            },
                        ],
                    });

                    w.x += 1;
                    w.y += 1;
                    if w.x >= src_width as i16 {
                        w.x = 0 - w.width as i16;
                    }
                    if w.y >= src_height as i16 {
                        w.y = 0 - w.height as i16;
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
                conn.flush();
                now = after;
            }
        }
    }
}
