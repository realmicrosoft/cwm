mod types;
mod helpers;
mod linkedlist;
mod setup;

use std::ffi::CStr;
use std::mem;
use std::num::NonZeroU32;
use std::os::raw::{c_char, c_int, c_ulong};
use std::ptr::{null, null_mut};
use std::time::SystemTime;
use stb_image::image::LoadResult;
use fast_image_resize as fr;
use libsex::bindings::{CWBorderPixel, CWHeight, CWWidth, CWX, CWY, Display, GL_COLOR_BUFFER_BIT, GL_PROJECTION, glClear, glClearColor, glLoadIdentity, glMatrixMode, glOrtho, glViewport, glXSwapBuffers, QueuedAlready, Screen, Window, XChangeWindowAttributes, XCompositeRedirectSubwindows, XConfigureWindow, XCreateWindowEvent, XDefaultScreenOfDisplay, XDestroyWindow, XEvent, XEventsQueued, XGetErrorText, XGetWindowAttributes, XMapWindow, XNextEvent, XOpenDisplay, XRootWindowOfScreen, XSetErrorHandler, XSetWindowAttributes, XSync, XWindowAttributes, XWindowChanges};
use crate::types::CumWindow;
use crate::helpers::{allow_input_passthrough, draw_x_window, rgba_to_bgra};
use crate::linkedlist::LinkedList;
use crate::setup::{setup_compositing, setup_desktop, setup_glx};

unsafe extern "C" fn error_handler(display: *mut libsex::bindings::Display, error_event: *mut libsex::bindings::XErrorEvent) -> c_int {
    unsafe {
        let mut buffer: [c_char; 256] = [0; 256];
        XGetErrorText(display, (*error_event).error_code as c_int, buffer.as_mut_ptr(), 256);
        println!("{}", CStr::from_ptr(buffer.as_ptr()).to_str().unwrap());
    }
    0
}

fn main() {
    unsafe {
        XSetErrorHandler(Some(error_handler));
    }
    let mut display: *mut Display = null_mut();
    let mut screen: *mut Screen = null_mut();
    let mut root: Window = 0;
    // get stuffz
    unsafe {
        display = XOpenDisplay(null());
        screen = XDefaultScreenOfDisplay(display);
        root = XRootWindowOfScreen(screen);
    }
    if display == null_mut() {
        println!("Could not open display");
        return;
    }
    if screen == null_mut() {
        println!("Could not get screen");
        return;
    }
    if root == 0 {
        println!("Could not get root window");
        return;
    }
    unsafe {
        XSync(display, 0);
    }

    // get dimensions
    let mut src_width = 0;
    let mut src_height = 0;

    unsafe {
        let mut attr: XWindowAttributes = XWindowAttributes{
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            border_width: 0,
            depth: 0,
            visual: null_mut(),
            root,
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
        XGetWindowAttributes(display, root, &mut attr);
        src_height = attr.height;
        src_width = attr.width;
    }
    unsafe {
        XSync(display, 0);
    }

    let mut windows = LinkedList::new();

    let mut accent_color = 0xFFFF0000;

    let (overlay_window, gc) = setup_compositing(display, root);
    unsafe {
        XSync(display, 0);
    }

    let (ctx, visual, fbconfigs, value, pict_format) =
        unsafe { setup_glx(display, overlay_window,src_width as u32, src_height as u32, screen) };

    unsafe {
        XSync(display, 0);
    }

    let desktop_id = unsafe { setup_desktop(display, gc, screen, pict_format, root, src_width as u16, src_height as u16) };

    unsafe {
        XSync(display, 0);
    }
    let mut now = SystemTime::now();
    let mut t = 0;
    let mut need_redraw = true;
    let mut dragging = false;

    let mut desktop_window = CumWindow {
        x: 0,
        y: 0,
        width: src_width as u16,
        height: src_height as u16,
        window_id: desktop_id,
        frame_id: 0,
        is_opening: false,
        animation_time: 0
    };

    let mut frame_windows: Vec<Window> = Vec::new();
    let mut windows_to_destroy: Vec<Window> = Vec::new();
    let mut windows_to_configure: Vec<CumWindow> = Vec::new();

    let mut cursor_x = 0;
    let mut cursor_y = 0;

    unsafe {
        XSync(display, 0);
    }

    loop {
        let events_pending = unsafe { XEventsQueued(display, QueuedAlready as c_int) };
        // if we have an event
        if events_pending > 0 {
            let mut event: XEvent = unsafe { mem::zeroed() };
            unsafe {
                XNextEvent(display, &mut event);
                match event.type_ {
                    CreateNotify => {
                        let ev = event.xcreatewindow;
                        println!("new window!");
                        // check the parent window to see if it's the root window
                        if root != ev.parent || desktop_id == ev.window || overlay_window == ev.window {
                            println!("nevermind, it is root, desktop, or overlay");
                        } else {
                            // check if this is a frame window
                            let mut found = false;
                            if frame_windows.contains(&ev.window) {
                                println!("nvm it's a frame window");
                                found = true;
                            }
                            if !found {
                                /*let centre_x = (src_width / 2) - (ev.width() / 2);
                                let centre_y = (src_height / 2) - (ev.height() / 2);
                                // change the main window to be in the centre of the screen
                                 */
                                // configure window
                                unsafe {
                                    XConfigureWindow(display, ev.window, CWX | CWY | CWWidth | CWHeight, &mut XWindowChanges{
                                        x: ev.x,
                                        y: ev.y,
                                        width: ev.width as c_int,
                                        height: ev.height as c_int,
                                        border_width: 0,
                                        sibling: 0,
                                        stack_mode: 0
                                    });
                                }
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
                                conn.flush().expect("flush failed!");
                                // add to the list of frames
                                frame_windows.push(frame_id);

                                 */
                                windows.push(CumWindow {
                                    window_id: ev.window,
                                    frame_id: 0,
                                    x: ev.x as i16,
                                    y: ev.y as i16 - 10,
                                    width: ev.width as u16,
                                    height: ev.height as u16,
                                    is_opening: false,
                                    animation_time: 0,
                                }).expect("failed to add window");
                                need_redraw = true;
                            }
                        }
                    }
                    DestroyNotify => {
                        let ev = event.xdestroywindow;
                        // add to the list of windows to destroy
                        windows_to_destroy.push(ev.window);
                        need_redraw = true;
                    }
                    ConfigureNotify => {
                        let ev = event.xconfigure;
                        // check if the window is the root window
                        if ev.window == root {
                            src_height = ev.height;
                            src_width = ev.width;
                            // todo: resize the sdl window (do we still need to do this?)
                        }
                        // add to windows to configure
                        windows_to_configure.push(CumWindow{
                            x: ev.x as i16,
                            y: ev.y as i16,
                            width: ev.width as u16,
                            height: ev.height as u16,
                            window_id: ev.window,
                            frame_id: 0,
                            is_opening: false,
                            animation_time: 0,
                        });
                        need_redraw = true;
                    }
                    Expose => {
                        // map window
                        unsafe {
                            let ev = event.xexpose;
                            XMapWindow(display, ev.window);
                        }
                        need_redraw = true;
                    }
                    ButtonPress => {
                        let ev = event.xbutton;
                        if ev.button == 1 {
                            // left click
                            println!("left click");
                        }
                    },
                    MotionNotify => {
                        let ev = event.xmotion;
                        // move cursor position
                        cursor_x = ev.x_root;
                        cursor_y = ev.y_root;
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
                unsafe {
                    glClearColor(0.29, 0.19, 0.3, 1.0);
                    glClear(GL_COLOR_BUFFER_BIT);

                    /*
                    glMatrixMode(GL_PROJECTION);
                    glLoadIdentity();
                    glOrtho(-1.0, 1.0, -1.0, 1.0, 1.0, 20.0);

                     */
                }

                // draw the desktop
                draw_x_window(desktop_window, display, visual, fbconfigs, value);

                let mut el = windows.index(0);
                let mut i = 0;
                while i < windows.len() {
                    if el.is_none(){
                        // if index is 0, there aren't any windows
                        if windows.len() > 0 {
                            el = windows.index(0);
                            i = 0;
                        } else {
                            break;
                        }
                    }
                    let w = unsafe { (*el.unwrap()).value };
                    // if we need to destroy this window, do so
                    if windows_to_destroy.contains(&w.window_id) {
                        unsafe {
                            XDestroyWindow(display, w.window_id);
                        }
                        windows.remove_at_index(i).expect("Error removing window");
                        el = windows.index(0);
                        i = 0;
                    } else if windows_to_configure.contains(&w) {
                        unsafe {
                            XConfigureWindow(display, w.window_id, CWX|CWY|CWWidth|CWHeight, &mut XWindowChanges{
                                x: w.x as c_int,
                                y: w.y as c_int,
                                width: w.width as c_int,
                                height: w.height as c_int,
                                border_width: 0,
                                sibling: 0,
                                stack_mode: 0
                            });
                        }
                        windows_to_configure.retain(|x| x != &w);
                        el = windows.index(0);
                        i = 0;
                    } else {
                        // set the window's border color
                        unsafe {
                            XChangeWindowAttributes(display, w.window_id, CWBorderPixel as c_ulong, &mut XSetWindowAttributes {
                                background_pixmap: 0,
                                background_pixel: 0,
                                border_pixmap: 0,
                                border_pixel: accent_color as c_ulong,
                                bit_gravity: 0,
                                win_gravity: 0,
                                backing_store: 0,
                                backing_planes: 0,
                                backing_pixel: 0,
                                save_under: 0,
                                event_mask: 0,
                                do_not_propagate_mask: 0,
                                override_redirect: 0,
                                colormap: 0,
                                cursor: 0,
                            });
                        }

                        // draw the window
                        draw_x_window(w, display, visual, fbconfigs, value);

                        el = windows.next_element(el.unwrap());
                        i += 1;
                    }
                }

                unsafe {
                    glXSwapBuffers(display, overlay_window);
                }
                now = after;
                need_redraw = false;
            }
        }
    }
}
