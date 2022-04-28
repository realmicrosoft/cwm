use smithay::backend::input::{InputBackend, InputEvent};
use smithay::backend::udev::UdevBackend;
use smithay::backend::winit::init;
use smithay::reexports::calloop::{EventLoop, LoopHandle};
use smithay::reexports::udev::Udev;
use smithay::reexports::wayland_server::Display;

mod types;
mod helpers;



/*
unsafe extern "C" fn error_handler(display: *mut Display, error_event: *mut XErrorEvent) -> c_int {
    unsafe { println!("X Error: {}", (*error_event).error_code); }
    0
}
 */

struct CumBackend {
    winit_backend: Option<smithay::backend::winit::WinitGraphicsBackend>,
    winit_input_backend: Option<smithay::backend::winit::WinitInputBackend>,
}

struct Cum {
    display: Display,
    event_loop: LoopHandle<'static, Cum>,
    backend: CumBackend,
}

fn main() {
    let mut event_loop: EventLoop<Cum> = EventLoop::try_new().expect("cant create event loop ):");
    let handle = event_loop.handle();
    let mut cum = Cum {
        display: Display::new(),
        event_loop: handle,
        backend: CumBackend {
            winit_backend: None,
            winit_input_backend: None,
        },
    };

    let (backend, input_backend) = init(None).unwrap();
    cum.backend.winit_backend = Some(backend);
    cum.backend.winit_input_backend = Some(input_backend);

    event_loop.run(
        std::time::Duration::from_millis(16),
        &mut cum,
        |_cum| {

        },
    ).expect("cant run event loop");
}