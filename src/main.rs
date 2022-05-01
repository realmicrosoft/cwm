mod types;
mod helpers;
mod linkedlist;
mod setup;

use std::ffi::{c_void, CStr};
use std::mem;
use std::num::NonZeroU32;
use std::os::raw::{c_char, c_int, c_ulong};
use std::ptr::{null, null_mut};
use std::time::SystemTime;
use stb_image::image::LoadResult;
use fast_image_resize as fr;
use libsex::bindings::{CWBorderPixel, CWHeight, CWWidth, CWX, CWY, Display, GL_ARRAY_BUFFER, GL_COLOR_BUFFER_BIT, GL_FALSE, GL_FLOAT, GL_FRAGMENT_SHADER, GL_MODELVIEW, GL_PROJECTION, GL_STATIC_DRAW, GL_VERTEX_SHADER, glAttachShader, glBindBuffer, glBindVertexArray, GLboolean, glBufferData, GLclampf, glClear, glClearColor, glCompileShader, glCreateProgram, glCreateShader, glDeleteTextures, glEnableVertexArrayAttrib, glGenBuffers, glGenVertexArrays, glGetAttribLocation, glGetUniformLocation, glLinkProgram, glLoadIdentity, glMatrixMode, glOrtho, glShaderSource, GLsizeiptr, GLuint, gluLookAt, glUseProgram, glVertexArrayAttribBinding, glVertexArrayAttribFormat, glViewport, glXSwapBuffers, QueuedAfterFlush, QueuedAlready, Screen, Window, XChangeWindowAttributes, XCompositeRedirectSubwindows, XConfigureWindow, XCreateWindowEvent, XDefaultScreenOfDisplay, XDestroyWindow, XEvent, XEventsQueued, XFlush, XGetErrorText, XGetWindowAttributes, XMapWindow, XNextEvent, XOpenDisplay, XRootWindowOfScreen, XSetErrorHandler, XSetWindowAttributes, XSync, XWindowAttributes, XWindowChanges};
use crate::types::CumWindow;
use crate::helpers::{allow_input_passthrough, draw_x_window, get_window_fb_config, rgba_to_bgra};
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
        println!("could not open display");
        return;
    }
    if screen == null_mut() {
        println!("could not get screen");
        return;
    }
    if root == 0 {
        println!("could not get root window");
        return;
    }
    unsafe {
        XSync(display, 0);
    }
    println!("display: {:?}", display);
    println!("screen: {:?}", screen);
    println!("root: {:?}", root);

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
    println!("source dimensions: {:?}x{:?}", src_width, src_height);

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

    let fbconfig = unsafe { get_window_fb_config(desktop_id, display, screen) };
    let desktop_window = CumWindow {
        x: 0,
        y: 0,
        width: src_width as u16,
        height: src_height as u16,
        window_id: desktop_id,
        frame_id: 0,
        fbconfig,
        is_opening: false,
        animation_time: 0
    };

    let mut frame_windows: Vec<Window> = Vec::new();
    let mut windows_to_destroy: Vec<Window> = Vec::new();
    let mut windows_to_configure: Vec<CumWindow> = Vec::new();
    let mut windows_to_open: Vec<Window> = Vec::new();

    let mut r= 0.0f64;
    let mut g= 0.0f64;
    let mut b= 0.0f64;

    let mut cursor_x = 0;
    let mut cursor_y = 0;

    unsafe {
        XSync(display, 0);
    }

    let mut event: XEvent = unsafe { mem::zeroed() };

    let mut shader_program = 0;

    unsafe {
        let vertex_source = "
#version 100
attribute vec2 position;
attribute vec3 color;
attribute vec2 texcoord;

varying vec3 Color;
varying vec2 Texcoord;


void main()
{

gl_Position = vec4(position, 0.0, 1.0);
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
gl_FragColor = texture2D(tex, Texcoord) * vec4(Color, 1.0);
}
                }";
        /*
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
        let tex_loc = glGetUniformLocation(shader_program, "tex".as_ptr() as *const i8);

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
*/
        glMatrixMode(GL_PROJECTION);
        glLoadIdentity();
        glOrtho(-1.25, 1.25, -1.25, 1.25, 1.0, 20.0);
        glMatrixMode(GL_MODELVIEW);
        glLoadIdentity();
        gluLookAt(0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0);
    }

    loop {
        //println!("loop");
        let events_pending = unsafe { XEventsQueued(display, QueuedAlready as c_int) };
        // if we have an event
        if events_pending > 0 {
            unsafe {
                XNextEvent(display, &mut event);
                match event.type_ {
                    CreateNotify => {
                        let ev = event.xcreatewindow;
                        println!("new window!");
                        // check the parent window to see if it's the root window
                        if root != ev.parent || desktop_id == ev.window || overlay_window == ev.window || ev.window == root {
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
                                let frame_id = conn.generate_id();
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
                                let fbconfig = get_window_fb_config(ev.window, display, screen);
                                windows.push(CumWindow {
                                    window_id: ev.window,
                                    frame_id: 0,
                                    x: ev.x as i16,
                                    y: ev.y as i16 - 10,
                                    width: ev.width as u16,
                                    height: ev.height as u16,
                                    is_opening: true,
                                    animation_time: 0,
                                    fbconfig,
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
                        let fbconfig = get_window_fb_config(ev.window, display, screen);
                        // add to windows to configure
                        windows_to_configure.push(CumWindow{
                            x: ev.x as i16,
                            y: ev.y as i16,
                            width: ev.width as u16,
                            height: ev.height as u16,
                            window_id: ev.window,
                            frame_id: 0,
                            fbconfig,
                            is_opening: true,
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
                        // add to windows to open
                        windows_to_open.push(event.xexpose.window);

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
                    _ => {
                        println!("unhandled event");
                        println!("{:?}", event.type_);
                    }
                }
            }
        }

        let after = SystemTime::now();
        if after.duration_since(now).unwrap().as_millis() > 10 {
            // generate the rainbow using a sine wave
            let frequency = 0.05;
            r = ((frequency * (t as f64) + 0.0).sin() * 127.0f64 + 128.0f64);
            g = ((frequency * (t as f64) + 2.0).sin() * 127.0f64 + 128.0f64);
            b = ((frequency * (t as f64) + 4.0).sin() * 127.0f64 + 128.0f64);

            accent_color = ((((r as u32) << 16) | ((g as u32) << 8) | (b as u32)) | 0xFF000000) as u32;
            t += 1;
            need_redraw = true;
            now = after;
        }

        if need_redraw {
            //println!("redrawing");
            unsafe {
                glClearColor((r/255.0f64) as GLclampf, (g/255.0f64) as GLclampf, (b/255.0f64) as GLclampf, 1.0);
                glClear(GL_COLOR_BUFFER_BIT);

                /*
                glMatrixMode(GL_PROJECTION);
                glLoadIdentity();
                glOrtho(-1.0, 1.0, -1.0, 1.0, 1.0, 20.0);

                 */
            }

            // draw the desktop
            draw_x_window(desktop_window, display, visual, value, shader_program);

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
                let mut w = unsafe { (*el.unwrap()).value };
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
                } else if windows_to_open.contains(&w.window_id) {
                    unsafe { (*el.unwrap()).value.is_opening = true; }
                    windows_to_open.retain(|x| x != &w.window_id);
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
                    if !w.is_opening {
                        draw_x_window(w, display, visual, value, shader_program);
                    }

                    el = windows.next_element(el.unwrap());
                    i += 1;
                }
            }

            unsafe {
                glXSwapBuffers(display, overlay_window);
            }
            need_redraw = false;
        }
    }
}
