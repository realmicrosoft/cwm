use x11::xlib::Window;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CumWindow {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub window_id: Window,
}