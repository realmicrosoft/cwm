mod types;
mod helpers;

use std::borrow::{Borrow, BorrowMut};
use std::cell::{Cell, RefCell, RefMut};
use std::detect::__is_feature_detected::sha;
use std::fs::File;
use std::io::Read;
use std::num::NonZeroU32;
use std::os::raw::{c_ulong};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use smithay::{backend, desktop, reexports, wayland};
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::{Frame, ImportDma, ImportDmaWl, ImportMem, ImportMemWl, Renderer, Texture, TextureFilter};
use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::backend::SwapBuffersError;
use smithay::desktop::space::SurfaceTree;
use smithay::reexports::calloop::{EventLoop, Interest, LoopHandle, LoopSignal, PostAction};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::Timer;
use smithay::reexports::{calloop, wayland_server};
use smithay::reexports::wayland_server::Display;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Size, Transform};
use smithay::wayland::compositor::{compositor_init, get_role, SurfaceData};
use smithay::wayland::output::{Mode, Output, PhysicalProperties};
use smithay::wayland::output::xdg::init_xdg_output_manager;
use smithay::wayland::shell::wlr_layer::{LayerShellRequest, LayerShellState};
use smithay::wayland::shell::xdg::{xdg_shell_init, XdgRequest};
use smithay::wayland::shell::xdg::decoration::{init_xdg_decoration_manager, XdgDecorationRequest};
use smithay::wayland::shm::init_shm_global;
use smithay::wayland::xdg_activation::{init_xdg_activation_global, XdgActivationEvent};
use crate::types::CumWindow;
use crate::helpers::rgba_to_bgra;

/*
unsafe extern "C" fn error_handler(display: *mut Display, error_event: *mut XErrorEvent) -> c_int {
    unsafe { println!("X Error: {}", (*error_event).error_code); }
    0
}
 */

struct CumTexture {
    width: u32,
    height: u32,
}

impl Texture for CumTexture {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }
}

struct CumFrame {}

impl Frame for CumFrame {
    type Error = SwapBuffersError;
    type TextureId = CumTexture;

    fn clear(&mut self, _color: [f32; 4], _damage: &[Rectangle<f64, Physical>]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn render_texture_from_to(
        &mut self,
        _texture: &Self::TextureId,
        _src: Rectangle<i32, Buffer>,
        _dst: Rectangle<f64, Physical>,
        _damage: &[Rectangle<f64, Physical>],
        _src_transform: Transform,
        _alpha: f32,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn transformation(&self) -> Transform {
        Transform::Normal
    }
}

struct CumRenderer {}

impl Renderer for CumRenderer {
    type Error = SwapBuffersError;
    type TextureId = CumTexture;
    type Frame = CumFrame;

    fn id(&self) -> usize {
        0
    }

    fn render<F, R>(
        &mut self,
        _size: Size<i32, Physical>,
        _dst_transform: Transform,
        rendering: F,
    ) -> Result<R, Self::Error>
        where
            F: FnOnce(&mut Self, &mut Self::Frame) -> R,
    {
        let mut frame = CumFrame {};
        Ok(rendering(self, &mut frame))
    }

    fn upscale_filter(&mut self, _filter: TextureFilter) -> Result<(), Self::Error> {
        Ok(())
    }

    fn downscale_filter(&mut self, _filter: TextureFilter) -> Result<(), Self::Error> {
        Ok(())
    }
}impl ImportMem for CumRenderer {
    fn import_memory(
        &mut self,
        _data: &[u8],
        _size: Size<i32, Buffer>,
        _flipped: bool,
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        unimplemented!()
    }

    fn update_memory(
        &mut self,
        _texture: &<Self as Renderer>::TextureId,
        _data: &[u8],
        _region: Rectangle<i32, Buffer>,
    ) -> Result<(), <Self as Renderer>::Error> {
        unimplemented!()
    }
}

impl ImportMemWl for CumRenderer {
    fn import_shm_buffer(
        &mut self,
        buffer: &reexports::wayland_server::protocol::wl_buffer::WlBuffer,
        surface: Option<&SurfaceData>,
        _damage: &[Rectangle<i32, Buffer>],
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        use smithay::wayland::shm::with_buffer_contents;
        let ret = with_buffer_contents(&buffer, |slice, data| {
            let offset = data.offset as u32;
            let width = data.width as u32;
            let height = data.height as u32;
            let stride = data.stride as u32;

            let mut x = 0;
            for h in 0..height {
                for w in 0..width {
                    x |= slice[(offset + w + h * stride) as usize];
                }
            }

            if let Some(data) = surface {
                data.data_map.insert_if_missing(|| Cell::new(0u8));
                data.data_map.get::<Cell<u8>>().unwrap().set(x);
            }

            (width, height)
        });

        match ret {
            Ok((width, height)) => Ok(CumTexture { width, height }),
            Err(e) => Err(SwapBuffersError::TemporaryFailure(Box::new(e))),
        }
    }
}

impl ImportDma for CumRenderer {
    fn import_dmabuf(
        &mut self,
        _dmabuf: &Dmabuf,
        _damage: Option<&[Rectangle<i32, Buffer>]>,
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        unimplemented!()
    }
}

impl ImportDmaWl for CumRenderer {}

struct Cum {
    pub initialised: bool,
    pub display: Rc<RefCell<Display>>,
    pub renderer: CumRenderer,
    pub handle: LoopHandle<'static, Cum>,
    pub space: desktop::space::Space,
    pub windows: Vec<CumWindow>,
}

struct ShellHandles {
    pub xdg_state: Arc<Mutex<wayland::shell::xdg::ShellState>>,
    pub layer_state: Arc<Mutex<LayerShellState>>,
}

impl Cum {
    pub fn init(&mut self, mut display: &mut Rc<RefCell<Display>>) {
        let mut display: RefMut<Display> = display.borrow_mut();
        self.handle.insert_source(
            Generic::from_fd(display.borrow().get_poll_fd(), Interest::READ, calloop::Mode::Level),
            move |_, _, state: &mut Cum| {
                let mut display = display.borrow_mut();
                match display.dispatch(std::time::Duration::from_millis(0), state) {
                    Ok(_) => { Ok(PostAction::Continue)}
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        Err(e)
                    }
                }
            },
        ).expect("failed to insert source ):");

        init_shm_global(&mut display, vec![], None);

        compositor_init(
            &mut display.borrow_mut(),
            move |surface, mut data| {
                on_commit_buffer_handler(&surface);
            },
            None
        );

        let (xdg_shell_state, _) = xdg_shell_init(
            &mut display.borrow_mut(),
            move |shell_event, mut data| {
                let state = &mut data.get::<Cum>().unwrap();
                match shell_event {
                    XdgRequest::NewClient { .. } => {}
                    XdgRequest::ClientPong { .. } => {}
                    XdgRequest::NewToplevel { .. } => {}
                    XdgRequest::NewPopup { .. } => {}
                    XdgRequest::Move { .. } => {}
                    XdgRequest::Resize { .. } => {}
                    XdgRequest::Grab { .. } => {}
                    XdgRequest::Maximize { .. } => {}
                    XdgRequest::UnMaximize { .. } => {}
                    XdgRequest::Fullscreen { .. } => {}
                    XdgRequest::UnFullscreen { .. } => {}
                    XdgRequest::Minimize { .. } => {}
                    XdgRequest::ShowWindowMenu { .. } => {}
                    XdgRequest::AckConfigure { .. } => {}
                    XdgRequest::RePosition { .. } => {}
                }
            },
            None
        );

        let (layer_shell_state, _) = wayland::shell::wlr_layer::wlr_layer_shell_init(
            &mut display.borrow_mut(),
            move |event, mut data| match event {
                LayerShellRequest::NewLayerSurface { .. } => {}
                LayerShellRequest::AckConfigure { .. } => {}
            },
            None
        );

        let shell_handles = ShellHandles {
            xdg_state: xdg_shell_state,
            layer_state: layer_shell_state,
        };

        init_xdg_output_manager(&mut display.borrow_mut(), None);
        init_xdg_activation_global(
            &mut display.borrow_mut(),
            |state, req, mut data| {
                let state = &mut data.get::<Cum>().unwrap();
                match req {
                    XdgActivationEvent::RequestActivation { .. } => {}
                    XdgActivationEvent::DestroyActivationRequest { .. } => {}
                }
            },
            None
        );
        init_xdg_decoration_manager(
            &mut display.borrow_mut(),
            |req, data| match req {
                XdgDecorationRequest::NewToplevelDecoration { .. } => {}
                XdgDecorationRequest::SetMode { .. } => {}
                XdgDecorationRequest::UnsetMode { .. } => {}
            },
            None
        );
    }
}

pub fn draw_dnd_icon(
    surface: reexports::wayland_server::protocol::wl_surface::WlSurface,
    location: impl Into<Point<i32, Logical>>,
) -> SurfaceTree {
    if get_role(&surface) != Some("dnd_icon") {
        println!("Not a dnd icon");
    }
    SurfaceTree {
        surface,
        position: location.into(),
        z_index: 100, /* Cursor should always be on-top */
    }
}

fn main() {
    let mut event_loop: EventLoop<Cum> = EventLoop::try_new().expect("cant create event loop ):");

    let handle = event_loop.handle();

    //let source = Timer::new().expect("failed 2 create timer ):");

    let mode = Mode {
        size: (1920, 1080).into(),
        refresh: 60_000,
    };

    let output = Output::new(
        "CHAOTIC WINDOW MANAGER".into(),
        PhysicalProperties{
            size: (0, 0).into(),
            subpixel: wayland_server::protocol::wl_output::Subpixel::HorizontalRgb,
            make: "cum".into(),
            model: "coom".into(),
        },
        None
    );
    output.change_current_state(
        Some(Mode {
            size: (0, 0).into(),
            refresh: 60_000,
        }),
        Some(wayland_server::protocol::wl_output::Transform::Normal),
        Some(wayland::output::Scale::Integer(1)),
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    let mut shared_data = Cum {
        initialised: false,
        display: Rc::new(RefCell::new(Display::new())),
        renderer: CumRenderer {},
        handle,
        space: desktop::space::Space::new(None),
        windows: Vec::new()
    };

    shared_data.space.borrow_mut().map_output(&output, (0,0));

    shared_data.init(shared_data.display.borrow_mut());

    event_loop.run(
        std::time::Duration::from_millis(69),
        &mut shared_data,
        |_shared_data| {
        },
    ).expect("failed to run event loop ):");
}
    /*

    // get dimensions
    let mut src_width = screen.width_in_pixels();
    let mut src_height = screen.height_in_pixels();

    let mut windows: Vec<types::CumWindow> = Vec::new();

    let mut accent_color = 0xFFFF0000;


    let mut now = SystemTime::now();
    let mut t = 0;
    let mut need_redraw = true;
    let mut window_active = 0;
    let mut dragging = false;

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
                    }
                    _ => {}
                }
            }

            let after = SystemTime::now();
            if after.duration_since(now).unwrap().as_millis() > 5 {
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
                conn.flush().expect("Error flushing");
                now = after;

                need_redraw = false;
            }
        }
    }
}


     */