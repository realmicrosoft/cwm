mod types;

use std::os::raw::c_int;
use std::ptr::{null, null_mut};
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

    let src_width = unsafe { XDisplayWidth(display, screen) };
    let src_height = unsafe { XDisplayHeight(display, screen) };

    let mut windows : Vec<types::CumWindow> = Vec::new();

    // get root window and set attributes so we receive events
    let mut set_attr: *mut XSetWindowAttributes = &mut XSetWindowAttributes{
        background_pixmap: 0,
        background_pixel: 0,
        border_pixmap: 0,
        border_pixel: 0,
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
        cursor: 0
    } as *mut XSetWindowAttributes;
    unsafe {
        (*set_attr).event_mask = SubstructureNotifyMask | SubstructureRedirectMask;
        XChangeWindowAttributes(display, root, CWEventMask, set_attr)
    };

    loop {
        unsafe { XNextEvent(display, ev); }
        if ev.is_null() {
            panic!("Failed to get next event");
        }
        // print windows
        println!("{:?}", windows);
        unsafe {
            match (*ev).type_ {
                CreateNotify => {
                    let create_notify: XCreateWindowEvent = (*ev).try_into().expect("Failed to convert event");
                    unsafe { XGetWindowAttributes(display, create_notify.window, attr); }
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
                _ => {}
            }
        }
        // for each window, move it by 1 up and 1 right
        for w in windows.iter_mut() {
            unsafe {
                XMoveWindow(display, w.window_id, w.x, w.y);
                XSynchronize(display, 0);
            }
            w.x += 1;
            w.y += 1;
            if w.x + w.width >= src_width {
                w.x = 0;
            }
            if w.y + w.height >= src_height {
                w.y = 0;
            }
        }
    }
}
