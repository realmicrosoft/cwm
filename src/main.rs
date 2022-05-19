mod types;
mod helpers;
mod linkedlist;
mod setup;

use std::borrow::Borrow;
use std::ffi::{c_void, CStr};
use std::mem;
use std::os::raw::{c_char, c_int, c_uint, c_ulong};
use std::ptr::{null, null_mut};
use std::time::SystemTime;
use fast_image_resize as fr;
use libsex::bindings::{AnyModifier, Button1Mask, ButtonPressMask, ButtonReleaseMask,
                       CopyFromParent, CWBackPixel, CWX, CWY,
                       Display, GL_ARRAY_BUFFER, GL_BLEND, GL_COLOR_BUFFER_BIT, GL_DEPTH_BUFFER_BIT,
                       GL_FALSE, GL_FLOAT, GL_FRAGMENT_SHADER, GL_ONE_MINUS_SRC_ALPHA,
                       GL_PROJECTION, GL_SRC_ALPHA, GL_STATIC_DRAW, GL_VERTEX_SHADER,
                       glAttachShader, glBindBuffer, glBindVertexArray, glBlendFunc, GLboolean,
                       glBufferData, GLclampf, glClear, glClearColor, glCompileShader,
                       glCreateProgram, glCreateShader, glEnable,
                       glEnableVertexArrayAttrib, glGenBuffers, glGenVertexArrays,
                       glGetAttribLocation, glLinkProgram, glLoadIdentity,
                       glMatrixMode, glShaderSource, GLsizeiptr, GLuint,
                       gluOrtho2D, glUseProgram, glVertexArrayAttribBinding,
                       glVertexArrayAttribFormat, glViewport, glXSwapBuffers, GrabModeAsync,
                       InputOutput, PictTypeDirect, PointerMotionMask,
                       QueuedAlready, Screen, Visual, Window, XConfigureWindow, XCreateWindow,
                       XDefaultScreenOfDisplay, XEvent, XEventsQueued, XFlush, XGetErrorText,
                       XGetWindowAttributes, XGrabButton, XMapWindow, XMoveWindow,
                       XNextEvent, XOpenDisplay, XQueryPointer, XRaiseWindow, XRenderFindVisualFormat,
                       XResizeWindow, XRootWindowOfScreen, XSendEvent, XSetErrorHandler,
                       XSetWindowAttributes, XSync, XWindowAttributes, XWindowChanges};


use crate::types::{CumWindow, XVelocity};
use crate::helpers::{allow_input_passthrough, draw_x_window, get_window_fb_config, redraw_desktop, rgba_to_bgra};
use crate::linkedlist::LinkedList;
use crate::setup::{setup_compositing, setup_desktop, setup_glx};

unsafe extern "C" fn error_handler(display: *mut Display, error_event: *mut libsex::bindings::XErrorEvent) -> c_int {
    let mut buffer: [c_char; 256] = [0; 256];
    XGetErrorText(display, (*error_event).error_code as c_int, buffer.as_mut_ptr(), 256);
    println!("{}", CStr::from_ptr(buffer.as_ptr()).to_str().unwrap());
    0
}

fn main() {
    unsafe {
        XSetErrorHandler(Some(error_handler));
    }
    let display: *mut Display;
    let screen: *mut Screen;
    let root: Window;
    // get stuffz
    unsafe {
        display = XOpenDisplay(null());
        if display.is_null() {
            println!("could not open display");
            return;
        }
        screen = XDefaultScreenOfDisplay(display);
        if screen.is_null() {
            println!("could not get screen");
            return;
        }
        root = XRootWindowOfScreen(screen);
        if root == 0 {
            println!("could not get root window");
            return;
        }
    }
    unsafe {
        XSync(display, 0);
    }
    println!("display: {:?}", display);
    println!("screen: {:?}", screen);
    println!("root: {:?}", root);

    // get dimensions
    let mut src_width: c_int;
    let mut src_height: c_int;

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
    println!("source dimensions: {:?}x{:?}", src_width, src_height);

    let mut windows = LinkedList::new();

    //let mut accent_color;

    let (overlay_window, gc) = setup_compositing(display, root);
    unsafe {
        XSync(display, 0);
    }

    let (_ctx, _visual, _fbconfigs, _value, pict_format) =
        unsafe { setup_glx(display, overlay_window,src_width as u32, src_height as u32, screen) };

    unsafe {
        XSync(display, 0);
    }

    let (desktop_id, desktop_picture) = setup_desktop(display, gc, screen, pict_format, root, src_width as u16, src_height as u16);
    println!("desktop id: {:?}", desktop_id);

    unsafe {
        XSync(display, 0);
    }
    let mut now = SystemTime::now();
    let mut t = 0;
    let mut need_redraw = true;

    let fbconfig = unsafe { get_window_fb_config(desktop_id, display, screen) };
    let desktop_window = CumWindow {
        x: 0,
        y: 0,
        width: src_width as u16,
        height: src_height as u16,
        window_id: desktop_id,
        frame_id: 0,
        fbconfig,
        hide: false,
        has_alpha: true,
        use_actual_position: false,
        event: None,
        velocity: XVelocity{
            x_speed: 0.0,
            last_x_location: 0,
        }
    };

    // rather use more memory than lose performance

    let mut frame_windows: Vec<Window> = Vec::new();
    //let mut frame_windows_to_pick_up: Vec<Window> = Vec::new();
    let mut windows_to_destroy: Vec<Window> = Vec::new();
    let mut windows_to_configure: Vec<Window> = Vec::new();
    let mut windows_to_finally_move: Vec<Window> = Vec::new();
    let mut windows_to_open: Vec<Window> = Vec::new();
    let mut windows_to_hide: Vec<Window> = Vec::new();

    let mut holding_window: Window = 0;
    let mut holding_window_x_offset: i32 = 0;
    let mut holding_window_y_offset: i32 = 0;
    let mut holding_window_x = 0;
    let mut holding_window_y = 0;
    //let mut last_window_pickup_time;

    let mut r= 0.0f64;
    let mut g= 0.0f64;
    let mut b= 0.0f64;

    //let mut cursor_x = 0;
    //let mut cursor_y = 0;

    unsafe {
        XGrabButton(display, 1, AnyModifier, root, 1, ButtonPressMask | ButtonReleaseMask | PointerMotionMask,
                    GrabModeAsync as c_int, GrabModeAsync as c_int, 0, 0);
        XSync(display, 0);
    }

    let mut event: XEvent = unsafe { mem::zeroed() };

    let shader_program;

    unsafe {
        let vertex_source = "
#version 100
attribute vec2 position;
attribute vec3 color;
attribute vec2 texcoord;

varying vec3 Color;
varying vec2 Texcoord;

uniform mat4 projection;

void main()
{

gl_Position = projection * vec4(position, 0.0, 1.0);
Color = color;
Texcoord = texcoord;
}
                  }";
        let frag_source = "
#version 100
precision mediump float;

varying vec3 Color;
varying vec2 Texcoord;

uniform sampler2D tex;


void main()
{
gl_FragColor = texture2D(tex, Texcoord);
}
                }";
        glCreateShader(GL_VERTEX_SHADER);
        glShaderSource(GL_VERTEX_SHADER, 1, vertex_source.as_ptr() as *const *const c_char, null());
        glCompileShader(GL_VERTEX_SHADER);
        glCreateShader(GL_FRAGMENT_SHADER);
        glShaderSource(GL_FRAGMENT_SHADER, 1, frag_source.as_ptr() as *const *const c_char, null());
        glCompileShader(GL_FRAGMENT_SHADER);

        shader_program = glCreateProgram();
        glAttachShader(shader_program, GL_VERTEX_SHADER);
        glAttachShader(shader_program, GL_FRAGMENT_SHADER);
        glLinkProgram(shader_program);
        glUseProgram(shader_program);

        let position_loc = glGetAttribLocation(shader_program, "position".as_ptr() as *const i8);
        let color_loc = glGetAttribLocation(shader_program, "color".as_ptr() as *const i8);
        let texcoord_loc = glGetAttribLocation(shader_program, "texcoord".as_ptr() as *const i8);

        let mut vao = 0;
        glGenVertexArrays(1, &mut vao);
        glBindVertexArray(vao);
        glEnableVertexArrayAttrib(vao, position_loc as GLuint);
        glEnableVertexArrayAttrib(vao, color_loc as GLuint);
        glEnableVertexArrayAttrib(vao, texcoord_loc as GLuint);
        glVertexArrayAttribBinding(vao, position_loc as GLuint, 0);
        glVertexArrayAttribBinding(vao, color_loc as GLuint, 0);
        glVertexArrayAttribBinding(vao, texcoord_loc as GLuint, 0);

        glVertexArrayAttribFormat(vao, position_loc as GLuint, 2, GL_FLOAT, GL_FALSE as GLboolean, 0);
        glVertexArrayAttribFormat(vao, color_loc as GLuint, 3, GL_FLOAT, GL_FALSE as GLboolean, 0);
        glVertexArrayAttribFormat(vao, texcoord_loc as GLuint, 2, GL_FLOAT, GL_FALSE as GLboolean, 0);

        let mut vbo = 0;
        glGenBuffers(1, &mut vbo);
        glBindBuffer(GL_ARRAY_BUFFER, vbo);

        let texture_coords = [
            0.0, 0.0,
            1.0, 0.0,
            0.0, 1.0,
            1.0, 1.0
        ];

        glBufferData(GL_ARRAY_BUFFER, (mem::size_of::<f32>() * texture_coords.len() as usize) as GLsizeiptr, texture_coords.as_ptr() as *const c_void, GL_STATIC_DRAW);

        glViewport(0, 0, src_width as i32, src_height as i32);
        glMatrixMode(GL_PROJECTION);
        glLoadIdentity();
        // make top left corner as origin
        //glOrtho(0.0, src_width as f64, src_height as f64, 0.0, -1.0, 1.0);
        gluOrtho2D(0.0, src_width as f64, src_height as f64, 0.0);
        glEnable(GL_BLEND);
        glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
    }

    loop {
        //println!("loop");
        unsafe {
            XFlush(display);
        }
        let events_pending = unsafe { XEventsQueued(display, QueuedAlready as c_int) };
        // if we have an event
        if events_pending > 0 {
            unsafe {
                XNextEvent(display, &mut event);
                match event.type_ {
                     16 => { // createnotify
                        let ev = event.xcreatewindow;
                        println!("new window!");
                        // check the parent window to see if it's the root window
                        if root != ev.parent || overlay_window == ev.window || root == ev.window || desktop_id == ev.window {
                            println!("nevermind, it is root, desktop, or overlay");
                        } else {
                            // check if this is a frame window
                            let mut found = false;
                            if frame_windows.contains(&ev.window) {
                                println!("nvm it's a frame window");
                                found = true;
                            }
                            if !found {
                                let centre_x = (src_width / 2) - (ev.width / 2);
                                let centre_y = (src_height / 2) - (ev.height / 2);
                                // change the main window to be in the centre of the screen
                                // configure window
                                XConfigureWindow(display, ev.window, CWX | CWY, &mut XWindowChanges{
                                    x: centre_x,
                                    y: centre_y,
                                    width: ev.width as c_int,
                                    height: ev.height as c_int,
                                    border_width: 1,
                                    sibling: 0,
                                    stack_mode: 0
                                });
                                // create the frame
                                let frame_id =
                                    XCreateWindow(display, root,
                                                  centre_x - 10, centre_y - 20,
                                                  ev.width as c_uint + 20, ev.height as c_uint + 25,
                                                  0, 24, InputOutput as c_uint,
                                                  CopyFromParent as *mut Visual, CWBackPixel as c_ulong, &mut XSetWindowAttributes{
                                        background_pixmap: 0,
                                        background_pixel: 0xefffffff,
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
                                    });
                                // map the window
                                XMapWindow(display, frame_id);

                                // raise the actual window
                                XRaiseWindow(display, ev.window);

                                // add to the list of frames
                                frame_windows.push(frame_id);

                                let fbconfig = get_window_fb_config(ev.window, display, screen);
                                let mut attribs : mem::MaybeUninit<XWindowAttributes> = mem::MaybeUninit::uninit();
                                XGetWindowAttributes(display, ev.window, attribs.as_mut_ptr());
                                let format = XRenderFindVisualFormat(display, attribs.assume_init().visual);
                                windows.push(CumWindow {
                                    window_id: ev.window,
                                    frame_id,
                                    x: ev.x as i32,
                                    y: ev.y as i32,// - 10,
                                    width: ev.width as u16,
                                    height: ev.height as u16,
                                    hide: true,
                                    has_alpha: ( (*format).type_ == PictTypeDirect as c_int && (*format).direct.alphaMask != 0 ),
                                    fbconfig,
                                    use_actual_position: true,
                                    event: None,
                                    velocity: XVelocity{
                                        x_speed: 0.0,
                                        last_x_location: ev.x as i32,
                                    }
                                }).expect("failed to add window");
                                need_redraw = true;
                            }
                        }
                    }
                    17 => { // destroynotify
                        let ev = event.xdestroywindow;
                        println!("destroyed window!");
                        // is this a frame window?
                        if frame_windows.contains(ev.window.borrow()) {
                            println!("nvm it's a frame window");
                            // remove from the list of frames
                            frame_windows.retain(|&x| x != ev.window);
                            need_redraw = true;
                        } else {
                            // add to the list of windows to destroy
                            windows_to_destroy.push(ev.window);
                            need_redraw = true;
                        }
                    }
                    22 => { // configure notify
                        let ev = event.xconfigure;
                        println!("configured window!");
                        // check if this is a frame window
                        if !frame_windows.contains(&ev.window) {
                            // check if the window is the root window
                            if ev.window == root {
                                src_height = ev.height;
                                src_width = ev.width;
                                // todo: resize the sdl window (do we still need to do this?)
                            }
                            /*let fbconfig = get_window_fb_config(ev.window, display, screen);
                            let mut attribs : mem::MaybeUninit<XWindowAttributes> = mem::MaybeUninit::uninit();
                            XGetWindowAttributes(display, ev.window, attribs.as_mut_ptr());
                            let format = XRenderFindVisualFormat(display, attribs.assume_init().visual);
                             */
                            // add to windows to configure
                            windows_to_configure.push(ev.window);
                            need_redraw = true;
                        }
                    }
                    23 => { // configure request
                        //let ev = event.xconfigure;
                        println!("configure request! (don't care)");
                        XFlush(display);
                    },
                    19 => { // map notify
                        // add to windows to open
                        println!("map notify");
                        if !frame_windows.contains(event.xmap.window.borrow()) {
                            windows_to_open.push(event.xmap.window);

                            need_redraw = true;
                        }
                        XFlush(display);
                    },
                    20 => { // map request
                        // add to windows to open
                        println!("map request");
                        XSendEvent(display, event.xmaprequest.window, 0, 0, &mut event);
                        XFlush(display);
                    },
                    18 => { // unmapnotify
                        // add to windows to close
                        println!("unmap notify");
                        if !frame_windows.contains(event.xmap.window.borrow()) {
                            windows_to_hide.push(event.xunmap.window);

                            need_redraw = true;
                        }
                    },
                    12 => { // expose
                        println!("expose");
                        // if window is desktop, redraw
                        if event.xexpose.window == desktop_id {
                            redraw_desktop(display, desktop_picture, desktop_id, pict_format, src_width as u32, src_height as u32);
                        } else {
                            // check if this is a frame window
                            if !frame_windows.contains(event.xmap.window.borrow()) {
                                // add to windows to open
                                windows_to_open.push(event.xexpose.window);
                            }
                        }
                        XMapWindow(display, event.xexpose.window);
                        need_redraw = true;
                    },
                    4 => { // button press
                        /*let ev = event.xbutton;
                        if ev.button == 1 {
                            // left click
                            println!("left click");
                            // is this a frame window?
                            if frame_windows.contains(ev.subwindow.borrow()) {
                                // add to the list of frames to pick up
                                frame_windows_to_pick_up.push(ev.subwindow);
                            }
                        }

                    },
                    5 => { // button release
                        let ev = event.xbutton;
                        if ev.button == 1 {
                            // left click
                            println!("left click");
                            // are we holding a window?
                            // add to the list of windows to move
                            windows_to_finally_move.push(holding_window);
                        }
                         */
                    },
                    6 => { // motionnotify
                        //let ev = event.xmotion;
                        // move cursor position
                        //cursor_x = ev.x_root;
                        //cursor_y = ev.y_root;
                        need_redraw = true;
                    },
                    25 => { // resize request (resize the frame but otherwise pass it on)
                        println!("resize request");
                        let ev = event.xresizerequest;
                        // check if this is a frame window
                        XSendEvent(display, ev.window, 0, 0, &mut event);
                        //if frame_windows.contains(ev.window.borrow()) {
                         //   XResizeWindow(display, ev.window, ev.width as c_uint, ev.height as c_uint);
                        //}
                        XFlush(display);
                        need_redraw = true;
                    },
                    27 => { // circulation request (just pass it along for now)
                        println!("circulation request");
                        let ev = event.xcirculaterequest;
                        XSendEvent(display, ev.window, 0, 0, &mut event);
                        XFlush(display);
                    },
                    30 => { // selection request (i don't know what this does so just pass it along)
                        println!("selection request");
                        let ev = event.xselectionrequest;
                        XSendEvent(display, ev.owner, 0, 0, &mut event);
                        XFlush(display);
                    },
                    _ => {
                        println!("unhandled event");
                        println!("{:?}", event.type_);
                    }
                }
            }
        }

        let after = SystemTime::now();
        if after.duration_since(now).unwrap().as_millis() > (1/60) as u128 {
            // generate the rainbow using a sine wave
            let frequency = 0.05;
            r = (frequency * (t as f64) + 0.0).sin() * 127.0f64 + 128.0f64;
            g = (frequency * (t as f64) + 2.0).sin() * 127.0f64 + 128.0f64;
            b = (frequency * (t as f64) + 4.0).sin() * 127.0f64 + 128.0f64;

            //accent_color = ((((r as u32) << 16) | ((g as u32) << 8) | (b as u32)) | 0xFF000000) as u32;
            t += 1;
            need_redraw = true;
            now = after;
        }

        if need_redraw {
            //println!("redrawing");
            unsafe {
                glClearColor((r/255.0f64) as GLclampf, (g/255.0f64) as GLclampf, (b/255.0f64) as GLclampf, 1.0);
                glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);

                /*
                glMatrixMode(GL_PROJECTION);
                glLoadIdentity();
                glOrtho(-1.0, 1.0, -1.0, 1.0, 1.0, 20.0);

                 */
            }

            // draw the desktop

            let mut el = windows.index(0);
            let mut i = 0;
            while i < windows.len() { // todo: this can be optimised with pointers
                if el.is_none(){
                    break;
                }
                let mut w = unsafe { (*el.unwrap()).value };
                // if we need to destroy this window, do so
                if !windows_to_destroy.is_empty() {
                    if windows_to_destroy.contains(&w.window_id) {
                        println!("completely destroying window");
                        windows.remove_at_index(i).expect("Error removing window");
                        windows_to_destroy.retain(|&x| x != w.window_id);
                        el = windows.index(0);
                        i = 0;
                    }
                } else if !windows_to_open.is_empty() {
                    if windows_to_open.contains(&w.window_id) {
                        println!("completely opening window");
                        let mut window = unsafe { (*el.unwrap()).value };
                        window.hide = false;
                        windows.change_element_at_index(i, window).expect("Error changing window");
                        windows_to_open.retain(|x| x != &w.window_id);
                    }
                } else if !windows_to_hide.is_empty() && windows_to_hide.contains(&w.window_id) {
                    println!("completely hiding window");
                    let mut window = unsafe { (*el.unwrap()).value };
                    window.hide = true;
                    windows.change_element_at_index(i, window).expect("Error changing window");
                    windows_to_hide.retain(|x| x != &w.window_id);
                }
                // for each window in windows to configure, check the window id
                let mut mouse_x = 0;
                let mut mouse_y = 0;
                let mut root_return: Window = 0;
                let mut child_return: Window = 0;
                let mut win_x_return: i32 = 0;
                let mut win_y_return: i32 = 0;
                let mut mask_return: c_uint = 0;

                unsafe {
                    XQueryPointer(display, root, &mut root_return,
                                  &mut child_return, &mut win_x_return, &mut win_y_return,
                                  &mut mouse_x, &mut mouse_y, &mut mask_return);
                    XSync(display, 0);
                }

                if holding_window != w.window_id {
                    if windows_to_configure.iter().any(|x| *x == w.window_id) {
                        // if the window is in the list, update the window

                        // get window attributes
                        let mut attr: XWindowAttributes = XWindowAttributes { // todo: we can make a macro function for empty xwindowattributes
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
                            screen,
                        };
                        unsafe {
                            XGetWindowAttributes(display, w.window_id, &mut attr);
                        }

                        w.x = attr.x;
                        w.y = attr.y;
                        w.width = attr.width as u16;
                        w.height = attr.height as u16;

                        unsafe {
                            XMoveWindow(display, w.frame_id, w.x as i32 - 10, w.y as i32 - 20);
                            XResizeWindow(display, w.frame_id, (w.width + 20) as c_uint, (w.height + 25) as c_uint);
                        }

                        let format = unsafe { XRenderFindVisualFormat(display, attr.visual) };

                        w.has_alpha = unsafe { ( (*format).type_ == PictTypeDirect as c_int && (*format).direct.alphaMask != 0 ) };
                        windows.change_element_at_index(i, w).expect("Error changing window");

                        windows_to_configure.retain(|x| x != &w.window_id);
                    }

                    // did the window get picked up?
                    if (child_return == w.frame_id || root_return == w.frame_id) && w.window_id != desktop_id && mask_return & Button1Mask as u32 != 0 {
                        println!("picking up window");
                        //last_window_pickup_time = SystemTime::now();
                        w.hide = false;
                        w.use_actual_position = false;
                        holding_window = w.window_id;

                        holding_window_x_offset = win_x_return as i32 - w.x;
                        holding_window_y_offset = win_y_return as i32 - w.y;
                        unsafe {
                            XRaiseWindow(display, w.frame_id);
                            XRaiseWindow(display, w.window_id);
                            XFlush(display);
                        }

                        windows.change_element_at_index(i, w).expect("Error changing window");
                    }
                }

                // is this a window being held?
                if holding_window == w.window_id && !w.hide {
                    //println!("holding window");
                    let mut dont_move = false;
                    if mask_return & Button1Mask as u32 == 0 {
                        // if so, move the window
                        println!("releasing window");
                        unsafe {
                            XMoveWindow(display, w.window_id, holding_window_x as c_int, holding_window_y as c_int);
                            XSync(display, 0);
                        }
                        w.use_actual_position = true;
                        w.x = holding_window_x;
                        w.y = holding_window_y;
                        windows.change_element_at_index(i, w).expect("Error changing window");
                        windows_to_finally_move.retain(|x| x != &w.window_id);
                        holding_window = 0;
                        dont_move = true;
                    }
                    if !dont_move {
                        // move the window to the cursor position (minus the offset)
                        w.x = mouse_x - holding_window_x_offset;
                        w.y = mouse_y - holding_window_y_offset;
                        holding_window_x = w.x;
                        holding_window_y = w.y;
                    }
                    draw_x_window(w, true, display, shader_program,
                                  false, 0, 0, r as u32, g as u32, b as u32);
                } else {
                    // draw the window
                    if !w.hide {
                        if w.window_id != desktop_id {
                            draw_x_window(w, true, display, shader_program,
                                          false, 0, 0, r as u32, g as u32, b as u32);
                        } else {
                            draw_x_window(w, false, display, shader_program,
                                          true, src_width as u32, src_height as u32,0,0,0);
                        }
                    }
                }

                if w.x != w.velocity.last_x_location { // todo: this needs to come before the window is drawn
                    w.velocity.x_speed -= (w.x - w.velocity.last_x_location) as f64 * 0.1;
                    w.velocity.last_x_location = w.x;
                    windows.change_element_at_index(i, w).expect("Error changing window");
                } else if w.velocity.x_speed != 0.0 {
                    w.velocity.x_speed *= 0.89;
                    windows.change_element_at_index(i, w).expect("Error changing window");
                }

                el = windows.next_element(el.unwrap());
                i += 1;

            }

            // we don't want to accidentally destroy a window, so clear the windows to destroy list
            windows_to_destroy.clear();
            // likewise, clear the windows to hide list
            windows_to_hide.clear();


            unsafe {
                glXSwapBuffers(display, overlay_window);
            }
            need_redraw = false;
        }
    }
}
