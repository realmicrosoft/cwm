use libsex::bindings::{GLXFBConfig, Window, XEvent};

#[derive(Clone, Copy)]
pub struct CumWindow {
    pub x: i32, // position to render the window at
    pub y: i32, // position to render the window at
    pub width: u16, // width of the window
    pub height: u16, // height of the window
    pub window_id: Window, // the window id
    pub frame_id: Window, // id of the frame window
    pub fbconfig: GLXFBConfig, // the framebuffer config
    pub hide: bool, // whether to draw the window
    pub has_alpha: bool, // whether the window has an alpha channel
    pub use_actual_position: bool, // should we render at the window's actual position, or the position we want it to be at?
    pub event: Option<XEvent>, // an associated event
}