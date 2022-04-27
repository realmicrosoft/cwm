mod types;

use std::borrow::Borrow;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_ulong};
use std::ptr::{null, null_mut};
use std::time::SystemTime;
use xcb::{x, Xid};
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
                //x::EventMask::EXPOSURE |
                x::EventMask::SUBSTRUCTURE_NOTIFY |
                //x::EventMask::SUBSTRUCTURE_REDIRECT
                x::EventMask::STRUCTURE_NOTIFY
            )
        ],});

    let checked = conn.check_request(cookie);
    if checked.is_err() {
        println!("Error setting event mask, is another window manager running?");
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
                        windows.push(CumWindow {
                            window_id: ev.window(),
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
                    w.x += 1;
                    w.y += 1;
                    if w.x >= src_width as i16 {
                        w.x = 0 - w.width as i16;
                    }
                    if w.y >= src_height as i16 {
                        w.y = 0 - w.height as i16;
                    }
                }
                conn.flush();
                now = after;
            }
        }
    }
}
