
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CumWindow {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub window_id: xcb::x::Window,
    pub pixmap_id: xcb::render::Picture,
    pub region_id: xcb::xfixes::Region,
    pub is_opening: bool,
    pub animation_time: i32,
}