mod types;
mod helpers;
mod linkedlist;
mod setup;

use std::num::NonZeroU32;
use std::os::raw::{c_ulong};
use std::time::SystemTime;
use stb_image::image::LoadResult;
use fast_image_resize as fr;
use libsex::bindings::glXSwapBuffers;
use xcb::{composite, Connection, glx, x, Xid};
use crate::types::CumWindow;
use crate::helpers::{allow_input_passthrough, draw_x_window, rgba_to_bgra};
use crate::setup::{setup_compositing, setup_desktop, setup_glx};

fn redraw_desktop(conn: &Connection, pic_id: xcb::render::Picture, desktop_pic_id: xcb::render::Picture, src_width: u16, src_height: u16) {
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

fn load_image(conn: &Connection, root: x::Window, gcon_id: x::Gcontext, pict_format: xcb::render::Pictformat, image_name: &str) -> xcb::render::Picture {// load the bg.png image
    let bg_image_load = stb_image::image::load(image_name);
    let bg_image = match bg_image_load {
        LoadResult::ImageU8(image) => image,
        LoadResult::ImageF32(_) => panic!("{} is not 8-bit", image_name),
        LoadResult::Error(e) => panic!("Error loading {}: {}", image_name, e),
    };

    // create a pixmap to draw on
    let bg_id = conn.generate_id();
    let cookie = conn.send_request_checked(&xcb::x::CreatePixmap {
        depth: 24,
        pid: bg_id,
        drawable: x::Drawable::Window(root),
        width: bg_image.width as u16,
        height: bg_image.height as u16,
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
        data: &rgba_to_bgra(&bg_image.data),
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error putting image on pixmap");
        println!("{:?}", checked);
    }

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
    pic_id
}

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
        ],
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error setting event mask, is another window manager running?");
    }

    let (overlay_window, pict_format) = setup_compositing(&conn, root);

    let (ctx, display, visual, fbconfigs, overlay) =
        unsafe { setup_glx(overlay_window.resource_id() as u64,src_width as u32, src_height as u32, screen_num) };

    let gcon_id: x::Gcontext = conn.generate_id();
    // create a graphics context
    let cookie = conn.send_request_checked(&xcb::x::CreateGc {
        cid: gcon_id,
        drawable: x::Drawable::Window(root),
        value_list: &[
            x::Gc::Foreground(accent_color),
            x::Gc::GraphicsExposures(true)
        ],
    });

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error creating graphics context");
    }

    let desktop_id = setup_desktop(&conn, screen.root_visual(), pict_format, gcon_id, root, src_width, src_height);

    // grab pointer for drawing the cursor
    let cookie = conn.send_request(&xcb::x::GrabPointer {
        owner_events: true,
        grab_window: root,
        event_mask: x::EventMask::POINTER_MOTION,
        pointer_mode: x::GrabMode::Async,
        keyboard_mode: x::GrabMode::Async,
        confine_to: x::Window::none(),
        cursor: x::Cursor::none(),
        time: x::CURRENT_TIME,
    });

    let cursor_image = load_image(&conn, root, gcon_id, pict_format, "cursor.png");

    let reply = conn.wait_for_reply(cookie).unwrap();
    if reply.status() != x::GrabStatus::Success {
        println!("Error grabbing pointer");
    }

    conn.flush().expect("flush failed!");

    let mut now = SystemTime::now();
    let mut t = 0;
    let mut need_redraw = true;
    let mut window_active = 0;
    let mut dragging = false;

    let mut desktop_window = CumWindow {
        x: 0,
        y: 0,
        width: src_width,
        height: src_height,
        window_id: desktop_id,
        frame_id: xcb::x::Window::none(),
        is_opening: false,
        animation_time: 0
    };

    let mut cursor_x = 0;
    let mut cursor_y = 0;

    loop {
        let event_pending = conn.poll_for_event();
        // if we have an event
        if let Ok(event_success) = event_pending {
            if event_success.is_some() {
                match event_success.unwrap() {
                    xcb::Event::X(x::Event::CreateNotify(ev)) => {
                        println!("new window!");
                        // check the parent window to see if it's the root window
                        if root != ev.parent() || desktop_id == ev.window() || overlay_window == ev.window() {
                            println!("nevermind, it is root, desktop, or overlay");
                        } else {
                            // check if this is a frame window
                            let mut found = false;
                            for w in windows.clone().iter() {
                                if w.frame_id == ev.window() {
                                    println!("nevermind, it is a frame");
                                    found = true;
                                    break;
                                }
                            }
                            if !found {
                                /*let centre_x = (src_width / 2) - (ev.width() / 2);
                                let centre_y = (src_height / 2) - (ev.height() / 2);
                                // change the main window to be in the centre of the screen
                                 */
                                conn.send_request(&xcb::x::ConfigureWindow {
                                    window: ev.window(),
                                    value_list: &[
                                        x::ConfigWindow::X(ev.x() as i32),
                                        x::ConfigWindow::Y(ev.y() as i32 - 10),
                                    ],
                                });
                                conn.flush().expect("flush failed!");
                                // create the frame
                                /*let frame_id = conn.generate_id();
                                conn.send_request(&xcb::x::CreateWindow {
                                    depth: 24,
                                    wid: frame_id,
                                    parent: root,
                                    x: ev.x() as i16,
                                    y: ev.y() as i16 - 10,
                                    width: ev.width() + 20 as u16,
                                    height: ev.height() + 20 as u16,
                                    border_width: 5,
                                    class: x::WindowClass::InputOutput,
                                    visual: screen.root_visual(),
                                    value_list: &[
                                        x::Cw::BackPixel(screen.white_pixel()),
                                        x::Cw::EventMask(x::EventMask::BUTTON_PRESS | x::EventMask::BUTTON_RELEASE | x::EventMask::EXPOSURE),
                                    ],
                                });
                                // map the frame
                                conn.send_request(&xcb::x::MapWindow {
                                    window: frame_id,
                                });

                                 */
                                conn.flush().expect("flush failed!");
                                windows.push(CumWindow {
                                    window_id: ev.window(),
                                    frame_id: x::Window::none(),
                                    x: ev.x() as i16,
                                    y: ev.y() as i16 - 10,
                                    width: ev.width(),
                                    height: ev.height(),
                                    is_opening: false,
                                    animation_time: 0,
                                });
                                need_redraw = true;
                            }
                        }
                    }
                    xcb::Event::X(x::Event::DestroyNotify(ev)) => {
                        // find the window in the list
                        need_redraw = true;
                        let mut found = false;
                        for w in windows.clone().iter_mut() {
                            if w.window_id == ev.window() {
                                found = true;
                                // destroy the frame
                                conn.send_request(&xcb::x::DestroyWindow {
                                    window: w.frame_id,
                                });
                                // remove the window
                                let mut i = 0;
                                for w in windows.clone().iter() {
                                    if w.window_id == ev.window() {
                                        break;
                                    }
                                    i += 1;
                                }
                                windows.remove(i);
                            }
                        }
                    }
                    xcb::Event::X(x::Event::ConfigureNotify(ev)) => {
                        // check if the window is the root window
                        if ev.window() == root {
                            src_height = ev.height();
                            src_width = ev.width();
                            // todo: resize the sdl window
                        }
                        let mut found = false;
                        for w in windows.iter_mut() {
                            if w.window_id == ev.window() {
                                found = true;
                                // update frame window position
                                conn.send_request(&xcb::x::ConfigureWindow {
                                    window: w.frame_id,
                                    value_list: &[
                                        x::ConfigWindow::X(ev.x() as i32 - 10),
                                        x::ConfigWindow::Y(ev.y() as i32 - 20),
                                        x::ConfigWindow::Width(ev.width() as u32 + 20),
                                        x::ConfigWindow::Height(ev.height() as u32 + 20),
                                    ],
                                });
                                w.x = ev.x();
                                w.y = ev.y();
                                w.width = ev.width();
                                w.height = ev.height();
                                need_redraw = true;
                                break;
                            }
                        }
                    }
                    xcb::Event::X(x::Event::Expose(ev)) => {
                        // map window
                        conn.send_request(&x::MapWindow {
                            window: ev.window(),
                        });
                        // if desktop window, copy pixmap to window
                        if ev.window() == desktop_id {
                            draw_x_window(&conn, desktop_window, display, visual, fbconfigs);
                        }
                        conn.flush().expect("Error flushing");
                        need_redraw = true;
                    }
                    xcb::Event::X(x::Event::ButtonPress(ev)) => {
                        if ev.detail() == 1 {
                            // left click
                            if ev.event() == root {
                                continue;
                            }
                            for (tmp, w) in windows.iter_mut().enumerate() {
                                if w.window_id == ev.event() {
                                    println!("{}", tmp);
                                    break;
                                }
                            }
                        }
                    },
                    xcb::Event::X(x::Event::MotionNotify(ev)) => {
                        // move cursor position
                        cursor_x = ev.root_x();
                        cursor_y = ev.root_y();
                    }
                    _ => {}
                }
            }

            let after = SystemTime::now();
            if after.duration_since(now).unwrap().as_millis() > (1/60) {
                // generate the rainbow using a sine wave
                let frequency = 0.05;
                let r = ((frequency * (t as f64) + 0.0).sin() * 127.0f64 + 128.0f64) as c_ulong;
                let g = ((frequency * (t as f64) + 2.0).sin() * 127.0f64 + 128.0f64) as c_ulong;
                let b = ((frequency * (t as f64) + 4.0).sin() * 127.0f64 + 128.0f64) as c_ulong;

                accent_color = (((r << 16) | (g << 8) | (b)) | 0xFF000000) as u32;
                t += 1;
                need_redraw = true;
            }

            if need_redraw {
                conn.flush().expect("Error flushing");

                // draw the desktop
                draw_x_window(&conn, desktop_window, display, visual, fbconfigs);

                for w in windows.iter_mut() {
                    // set the window's border color
                    conn.send_request(&x::ChangeWindowAttributes {
                        window: w.frame_id,
                        value_list: &[
                            x::Cw::BorderPixel(accent_color as u32),
                        ],
                    });

                    conn.flush().expect("Error flushing");

                    // draw the window
                    draw_x_window(&conn, *w, display, visual, fbconfigs);
                }

                unsafe {
                    glXSwapBuffers(display, overlay_window.resource_id() as u64);
                }

                conn.flush().expect("Error flushing");
                now = after;

                need_redraw = false;
            }
        }
    }
}
