use slog::{crit, o, Drain};

static POSSIBLE_BACKENDS: &[&str] = &[
    "--winit : Run anvil as a X11 or Wayland client using winit.",
    "--tty-udev : Run anvil as a tty udev client (requires root if without logind).",
    "--x11 : Run anvil as an X11 client.",
];

fn main() {
    // dummy logger
    let dummy_log = slog::Logger::root(slog::Discard, o!());
    // A logger facility, here we use the terminal here
    let log = if std::env::var("ANVIL_MUTEX_LOG").is_ok() {
        slog::Logger::root(std::sync::Mutex::new(slog_term::term_full().fuse()).fuse(), o!())
    } else {
        slog::Logger::root(
            slog_async::Async::default(slog_term::term_full().fuse()).fuse(),
            o!(),
        )
    };

    let _guard = slog_scope::set_global_logger(log.clone());
    slog_stdlog::init().expect("Could not setup log backend");

    let arg = ::std::env::args().nth(1);
    match arg.as_ref().map(|s| &s[..]) {
        Some("--winit") => {
            slog::info!(log, "Starting anvil with winit backend");
            CWM::winit::run_winit(dummy_log);
        }
        Some("--tty-udev") => {
            slog::info!(log, "Starting anvil on a tty using udev");
            CWM::udev::run_udev(dummy_log);
        }
        Some("--x11") => {
            slog::info!(log, "Starting anvil with x11 backend");
            CWM::x11::run_x11(dummy_log);
        }
        Some(other) => {
            crit!(log, "Unknown backend: {}", other);
        }
        None => {
            println!("USAGE: anvil --backend");
            println!();
            println!("Possible backends are:");
            for b in POSSIBLE_BACKENDS {
                println!("\t{}", b);
            }
        }
    }
}
