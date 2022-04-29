use std::borrow::{Borrow, BorrowMut};
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;
use std::sync::Mutex;
use smithay::backend::drm;
use smithay::backend::drm::DrmDevice;
use smithay::backend::input::{InputBackend, InputEvent};
use smithay::backend::session::auto::AutoSession;
use smithay::backend::udev::UdevBackend;
use smithay::backend::winit::init;
use smithay::desktop::Space;
use smithay::reexports;
use smithay::reexports::calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::udev::Udev;
use smithay::reexports::wayland_server::Display;
use smithay::utils::{Logical, Point};
use smithay::wayland::seat::CursorImageStatus;
use crate::drmb::UdevData;

mod types;
mod helpers;
mod winit;
mod render;
mod drawing;
mod drmb;
mod cursor;


/*
unsafe extern "C" fn error_handler(display: *mut Display, error_event: *mut XErrorEvent) -> c_int {
    unsafe { println!("X Error: {}", (*error_event).error_code); }
    0
}
 */

#[derive(Clone)]
struct FdWrapper {
    file: Rc<File>,
}

impl AsRawFd for FdWrapper {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

pub struct CumBackend {
    winit_backend: Option<winit::WinitData>,
    udev_backend: Option<Rc<RefCell<UdevData>>>,
    seat_name: Option<String>,
}

pub struct Cum {
    display: Rc<RefCell<Display>>,
    event_loop: LoopHandle<'static, Cum>,
    space: Rc<RefCell<Space>>,
    data: CumBackend,
    start_time: std::time::Instant,
    pointer_location: Point<f64, Logical>,
    cursor_status: Mutex<CursorImageStatus>,
}

impl Cum {
    pub fn process_input_event_windowed<B: InputBackend>(&mut self, event: InputEvent<B>, output_name: &str) {
        println!("keyboard event!");
    }
}

impl Clone for Cum {
    fn clone(&self) -> Self {
        // gotta do some special stuff for data and cursor_status
        let new_cursor_status = self.cursor_status.lock().unwrap().clone();
        let new_data = CumBackend{
            winit_backend: self.data.winit_backend.clone(),
            udev_backend: self.data.udev_backend.clone(),
            seat_name: self.data.seat_name.clone(),
        };
        Cum {
            display: self.display.clone(),
            event_loop: self.event_loop.clone(),
            space: self.space.clone(),
            data: new_data,
            start_time: self.start_time.clone(),
            pointer_location: self.pointer_location.clone(),
            cursor_status: Mutex::new(new_cursor_status),
        }
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
    winit::run_winit();
}
    /*let mut event_loop: EventLoop<Cum> = EventLoop::try_new().expect("cant create event loop ):");
    let handle = event_loop.handle();

    let mut display = Display::new();

    let socket = display.add_socket_auto();

    let display_fd = display.get_poll_fd();

    handle.insert_source(
        Generic::from_fd(display_fd, Interest::READ, Mode::Level),
        move |_, _, state: &mut Cum| {
            let mut display = state.display.clone();
            let mut display = (*display).borrow_mut();
            match display.dispatch(std::time::Duration::from_millis(0), state) {
                Ok(_) => Ok(PostAction::Continue),
                Err(e) => {
                    println!("display error");
                    Err(e)
                }
            }
        },
    ).expect("cant insert display fd");

    let mut cum = Cum {
        display: Rc::new(RefCell::new(display)),
        event_loop: handle,
        backend: CumBackend {
            winit_backend: None,
            winit_input_backend: None,
        },
    };

    event_loop.run(
        std::time::Duration::from_millis(16),
        &mut cum,
        |_cum| {
        },
<<<<<<< HEAD
    ).expect("cant run event loop");
=======
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
                        if root != ev.parent() || desktop_id == ev.window() {
                            println!("nevermind, it is root or desktop");
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
                                let centre_x = (src_width / 2) - (ev.width() / 2);
                                let centre_y = (src_height / 2) - (ev.height() / 2);
                                // change the main window to be in the centre of the screen
                                conn.send_request(&xcb::x::ConfigureWindow {
                                    window: ev.window(),
                                    value_list: &[
                                        x::ConfigWindow::X(centre_x as i32),
                                        x::ConfigWindow::Y(centre_y as i32),
                                    ],
                                });
                                conn.flush().expect("flush failed!");
                                // get pixmap for window
                                let p_id = conn.generate_id();
                                conn.send_request(&xcb::render::CreatePicture {
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
                                    dx: -(centre_x as i16),
                                    dy: -(centre_y as i16),
                                });
                                conn.send_request(&xcb::xfixes::SetPictureClipRegion {
                                    picture: p_id,
                                    region: r_id,
                                    x_origin: 0,
                                    y_origin: 0,
                                });
                                // create the frame
                                let frame_id = conn.generate_id();
                                conn.send_request(&xcb::x::CreateWindow {
                                    depth: 24,
                                    wid: frame_id,
                                    parent: root,
                                    x: centre_x as i16,
                                    y: centre_y as i16 - 10,
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
                                windows.push(CumWindow {
                                    window_id: ev.window(),
                                    frame_id,
                                    pixmap_id: p_id,
                                    region_id: r_id,
                                    x: centre_x as i16,
                                    y: centre_y as i16,
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
                        if !found {
                            // window not found, ignore
                            continue;
                        }
                    }
                    xcb::Event::X(x::Event::Expose(ev)) => {
                        // map window
                        conn.send_request(&x::MapWindow {
                            window: ev.window(),
                        });
                        // if desktop window, copy pixmap to window
                        if ev.window() == desktop_id {
                            redraw_desktop(&conn, pic_id, desktop_pic_id, src_width, src_height);
                        }
                        conn.flush().expect("Error flushing");
                        need_redraw = true;
                    }
                    xcb::Event::X(x::Event::ButtonPress(ev)) => {
                        if ev.detail() == 1 {
                            // left click
                            let mut tmp = 0;
                            if ev.event() == root {
                                continue;
                            }
                            for w in windows.iter_mut() {
                                if w.window_id == ev.event() {
                                    println!("{}", tmp);
                                    break;
                                }
                                tmp += 1;
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
                // get root pixmap
                let r_id = conn.generate_id();
                conn.send_request(&xcb::render::CreatePicture {
                    pid: r_id,
                    drawable: x::Drawable::Window(root),
                    format: pict_format,
                    value_list: &[
                        xcb::render::Cp::SubwindowMode(xcb::x::SubwindowMode::IncludeInferiors),
                    ],
                });

                // get desktop pixmap
                let d_id = conn.generate_id();
                conn.send_request(&xcb::render::CreatePicture {
                    pid: d_id,
                    drawable: x::Drawable::Window(desktop_id),
                    format: pict_format,
                    value_list: &[
                        xcb::render::Cp::SubwindowMode(xcb::x::SubwindowMode::IncludeInferiors),
                    ],
                });
                conn.send_request(&xcb::render::Composite {
                    op: xcb::render::PictOp::Over,
                    src: d_id,
                    mask: xcb::render::Picture::none(),
                    dst: r_id,
                    src_x: 0,
                    src_y: 0,
                    mask_x: 0,
                    mask_y: 0,
                    dst_x: 0,
                    dst_y: 0,
                    width: src_width as u16,
                    height: src_height as u16,
                });

                // for each window, move it by 1 up and 1 right
                for w in windows.iter_mut() {
                    // set the window's border color
                    conn.send_request(&x::ChangeWindowAttributes {
                        window: w.frame_id,
                        value_list: &[
                            x::Cw::BorderPixel(accent_color as u32),
                        ],
                    });

                    conn.flush().expect("Error flushing");
                    // get window pixmap
                    let p_id = conn.generate_id();
                    conn.send_request(&xcb::render::CreatePicture {
                        pid: p_id,
                        drawable: x::Drawable::Window(w.window_id),
                        format: pict_format,
                        value_list: &[
                            xcb::render::Cp::SubwindowMode(xcb::x::SubwindowMode::IncludeInferiors),
                        ],
                    });
                    // get frame pixmap
                    let f_id = conn.generate_id();
                    conn.send_request(&xcb::render::CreatePicture {
                        pid: f_id,
                        drawable: x::Drawable::Window(w.frame_id),
                        format: pict_format,
                        value_list: &[
                            xcb::render::Cp::SubwindowMode(xcb::x::SubwindowMode::IncludeInferiors),
                        ],
                    });
                    // composite the frame
                    conn.send_request(&xcb::render::Composite {
                        op: xcb::render::PictOp::Over,
                        src: f_id,
                        mask: xcb::render::Picture::none(),
                        dst: r_id,
                        src_x: -5,
                        src_y: -5,
                        mask_x: 0,
                        mask_y: 0,
                        dst_x: w.x - 10 - 5,
                        dst_y: w.y - 20 - 5,
                        width: w.width as u16 + 20 + 10,
                        height: w.height as u16 + 20 + 10,
                    });

                    // composite render pixmap onto window
                    if desktop_id != w.window_id {
                        conn.send_request(&xcb::render::Composite {
                            op: xcb::render::PictOp::Over,
                            src: p_id,
                            mask: xcb::render::Picture::none(),
                            dst: r_id,
                            src_x: -1,
                            src_y: -1,
                            mask_x: 0,
                            mask_y: 0,
                            dst_x: w.x - 1,
                            dst_y: w.y - 1,
                            width: w.width as u16 + 2,
                            height: w.height as u16 + 2,
                        });
                    } else {}
                }

                // composite cursor onto root
                conn.send_request(&xcb::render::Composite {
                    op: xcb::render::PictOp::Over,
                    src: cursor_image,
                    mask: xcb::render::Picture::none(),
                    dst: r_id,
                    src_x: 0,
                    src_y: 0,
                    mask_x: 0,
                    mask_y: 0,
                    dst_x: cursor_x,
                    dst_y: cursor_y,
                    width: 16 as u16,
                    height: 16 as u16,
                });

                conn.flush().expect("Error flushing");
                now = after;

                need_redraw = false;
            }
        }
    }
>>>>>>> mommy
}

     */