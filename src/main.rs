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
    ).expect("cant run event loop");
}

     */