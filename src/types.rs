use libsex::bindings::{GLXFBConfig, Window};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CumWindow {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub window_id: Window,
    pub frame_id: Window,
    pub fbconfig: GLXFBConfig,
    pub is_opening: bool,
    pub animation_time: i32,
}