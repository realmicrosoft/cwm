mod types;
use std::os::raw::{c_int, c_ulong};
use std::ptr::{null, null_mut};
use std::time::SystemTime;
use x11::xlib::*;
use crate::types::CumWindow;


fn main() {
    let display = unsafe { XOpenDisplay(null()) }; // get ready for a ton of unsafe code! (:
    if display.is_null() {
        panic!("Failed to open display");
    }
    let screen = unsafe { XDefaultScreen(display) };
    let mut attr: *mut XWindowAttributes = &mut XWindowAttributes{
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        border_width: 0,
        depth: 0,
        visual: null_mut() as *mut Visual,
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
        screen: null_mut() as *mut Screen,
    } as *mut XWindowAttributes;
    let ev : *mut XEvent = &mut XEvent{
        pad: [0; 24],
    } as *mut XEvent;
    let root = unsafe { XDefaultRootWindow(display) };

    let mut src_width = unsafe { XDisplayWidth(display, screen) };
    let mut src_height = unsafe { XDisplayHeight(display, screen) };

    let mut windows : Vec<types::CumWindow> = Vec::new();

    // visual
    let visual = unsafe { XDefaultVisual(display, screen) };

    // colormap
    let mut colormap = unsafe { XCreateColormap(display, root, visual, AllocNone) };
    // install colormap
    unsafe { XInstallColormap(display, colormap) };

    let mut accent_color = 0xFFFF0000;

    // get root window and set attributes so we receive events
    let mut set_attr: *mut XSetWindowAttributes = &mut XSetWindowAttributes{
        background_pixmap: 0,
        background_pixel: 0,
        border_pixmap: 0,
        border_pixel: accent_color,
        bit_gravity: 0,
        win_gravity: 0,
        backing_store: 0,
        backing_planes: 0,
        backing_pixel: 0,
        save_under: 0,
        event_mask: 0,
        do_not_propagate_mask: 0,
        override_redirect: 0,
        colormap,
        cursor: 0
    } as *mut XSetWindowAttributes;
    unsafe {
        (*set_attr).event_mask = StructureNotifyMask|SubstructureNotifyMask|EnterWindowMask|LeaveWindowMask;
        XChangeWindowAttributes(display, root, CWEventMask, set_attr)
    };

    let mut now = SystemTime::now();
    let mut t = 0;


    loop {
        let event_pending = unsafe { XPending(display) };
        if (event_pending > 0) {
            unsafe { XNextEvent(display, ev); }
            if ev.is_null() {
                panic!("Failed to get next event");
            }
            unsafe {
                match (*ev).type_ {
                    CreateNotify => {
                        // check the parent window to see if it's the root window
                        let create_notify: XCreateWindowEvent = (*ev).try_into().expect("Failed to convert event");
                        unsafe {
                            //XSelectInput(display, create_notify.window, (*set_attr).event_mask);
                            XSetWindowBorderWidth(display, create_notify.window, 5);
                            // set border color
                            XChangeWindowAttributes(display, create_notify.window, CWBorderPixel, set_attr);
                            XSetWindowBorder(display, create_notify.window, 0xFFFFFF00);
                        };
                        if root != create_notify.parent {
                            // child window, ignore
                            continue;
                        }
                        windows.push(CumWindow {
                            window_id: create_notify.window,
                            x: create_notify.x,
                            y: create_notify.y,
                            width: create_notify.width,
                            height: create_notify.height,
                        });
                    },
                    DestroyNotify => {
                        let destroy_notify: XDestroyWindowEvent = (*ev).try_into().expect("Failed to convert event");
                        windows.retain(|w| w.window_id != destroy_notify.window);
                    },
                    ConfigureNotify => {
                        let configure_notify: XConfigureEvent = (*ev).try_into().expect("Failed to convert event");
                        // check if the window is the root window
                        if configure_notify.window == root {
                            src_height = configure_notify.height;
                            src_width = configure_notify.width;
                        }
                        let mut found = false;
                        for w in windows.iter_mut() {
                            if w.window_id == configure_notify.window {
                                found = true;
                                w.x = configure_notify.x;
                                w.y = configure_notify.y;
                                w.width = configure_notify.width;
                                w.height = configure_notify.height;
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
            // print windows
            /*println!("{:?}", windows);
            println!("{:?}", src_width);
            println!("{:?}", src_height);
             */
        }
        let after = SystemTime::now();
        unsafe {
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
                    unsafe {
                        XMoveWindow(display, w.window_id, w.x, w.y);
                        XSynchronize(display, 0);
                        XSetWindowBorder(display, w.window_id, accent_color);
                    }
                    w.x += 1;
                    w.y += 1;
                    if w.x >= src_width {
                        w.x = 0 - w.width;
                    }
                    if w.y >= src_height {
                        w.y = 0 - w.height;
                    }
                }
                now = after;
            }
        }
    }
}
