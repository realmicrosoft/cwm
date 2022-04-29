#![warn(rust_2018_idioms)]

#[macro_use]
extern crate slog;

pub mod cursor;
pub mod drawing;
pub mod input_handler;
pub mod render;
pub mod shell;
pub mod state;
pub mod udev;
pub mod winit;
pub mod x11;

pub mod xwayland;

pub use state::AnvilState;
