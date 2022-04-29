use std::{
    borrow::Cow,
    cell::RefCell,
    collections::hash_map::{Entry, HashMap},
    os::unix::io::{AsRawFd, RawFd},
    path::PathBuf,
    rc::Rc,
    sync::atomic::Ordering,
    time::Duration,
};
use std::borrow::{Borrow};
use std::sync::Mutex;

use slog::Logger;

use crate::{drawing::*, Cum, CumBackend, drmb};
use smithay::{
    backend::renderer::{ImportDma, ImportEgl},
    wayland::dmabuf::init_dmabuf_global,
};
use smithay::{
    backend::{
        drm::{DrmDevice, DrmError, DrmEvent, DrmNode, GbmBufferedSurface, NodeType},
        egl::{EGLContext, EGLDevice, EGLDisplay},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            gles2::Gles2Renderbuffer,
            multigpu::{egl::EglGlesBackend, GpuManager, MultiRenderer, MultiTexture},
            Bind, Frame, ImportMem, Renderer,
        },
        session::{auto::AutoSession, Session, Signal as SessionSignal},
        udev::{all_gpus, primary_gpu, UdevBackend, UdevEvent},
        SwapBuffersError,
    },
    desktop::space::{RenderError, Space, SurfaceTree},
    reexports::{
        calloop::{
            timer::{Timer, TimerHandle},
            Dispatcher, EventLoop, LoopHandle, RegistrationToken,
        },
        drm::{
            self,
            control::{
                connector::{Info as ConnectorInfo, State as ConnectorState},
                crtc,
                encoder::Info as EncoderInfo,
                Device as ControlDevice,
            },
        },
        gbm::Device as GbmDevice,
        input::Libinput,
        nix::{fcntl::OFlag, sys::stat::dev_t},
        wayland_server::{
            protocol::{wl_output, wl_surface},
            Display, Global,
        },
    },
    utils::{
        signaling::{Linkable, SignalToken, Signaler},
        Logical, Point, Rectangle, Transform,
    },
    wayland::{
        output::{Mode, Output, PhysicalProperties},
        seat::CursorImageStatus,
    },
};
use smithay::backend::renderer::gles2::Gles2Renderer;
use smithay::backend::session::Signal;
use smithay::reexports::drm::control::crtc::Handle;
use smithay::reexports::gbm::Device;

type UdevRenderer<'a> = MultiRenderer<'a, 'a, EglGlesBackend, EglGlesBackend, Gles2Renderbuffer>;
smithay::custom_elements! {
    pub CustomElem<=UdevRenderer<'_>>;
    SurfaceTree=SurfaceTree,
    PointerElement=PointerElement::<MultiTexture>,
}

#[derive(Copy, Clone)]
pub struct SessionFd(RawFd);
impl AsRawFd for SessionFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

#[derive(Debug, PartialEq)]
struct UdevOutputId {
    device_id: DrmNode,
    crtc: crtc::Handle,
}

pub struct UdevData {
    pub session: AutoSession,
    primary_gpu: DrmNode,
    gpus: GpuManager<EglGlesBackend>,
    backends: HashMap<DrmNode, BackendData>,
    pointer_images: Vec<(xcursor::parser::Image, MultiTexture)>,
    signaler: Signaler<SessionSignal>,
    pointer_image: crate::cursor::Cursor,
    render_timer: TimerHandle<(DrmNode, crtc::Handle)>,
    logger: slog::Logger,
}

pub fn run_udev(log: Logger) {
    let mut event_loop = EventLoop::try_new().unwrap();
    let display = Rc::new(RefCell::new(Display::new()));

    /*
     * Initialize session
     */
    let (session, notifier) = match AutoSession::new(log.clone()) {
        Some(ret) => ret,
        None => {
            println!( "Could not initialize a session");
            return;
        }
    };
    let session_signal = notifier.signaler();

    /*
     * Initialize the compositor
     */
    let primary_gpu = if let Ok(var) = std::env::var("ANVIL_DRM_DEVICE") {
        DrmNode::from_path(var).expect("Invalid drm device path")
    } else {
        primary_gpu(&session.seat())
            .unwrap()
            .and_then(|x| DrmNode::from_path(x).ok()?.node_with_type(NodeType::Render)?.ok())
            .unwrap_or_else(|| {
                all_gpus(&session.seat())
                    .unwrap()
                    .into_iter()
                    .find_map(|x| DrmNode::from_path(x).ok())
                    .expect("No GPU!")
            })
    };
    println!("Using {} as primary gpu.", primary_gpu);

        let mut gpus = GpuManager::new(EglGlesBackend, log.clone()).unwrap();
        let mut renderer = gpus
        .renderer::<Gles2Renderbuffer>(&primary_gpu, &primary_gpu)
        .unwrap();

    {
        println!(
            "Trying to initialize EGL Hardware Acceleration via {:?}", primary_gpu
        );
        if renderer.bind_wl_display(&*display.borrow_mut()).is_ok() {
            println!("EGL hardware-acceleration enabled");
        }
    }
        let dmabuf_formats = renderer.dmabuf_formats().cloned().collect::<Vec<_>>();

    // setup the timer
    let timer = Timer::new().unwrap();

    let seat_name = session.seat().clone();
    let data = UdevData {
        session,
        primary_gpu,
        gpus,
        backends: HashMap::new(),
        signaler: session_signal.clone(),
        pointer_image: crate::cursor::Cursor::load(&log),
        pointer_images: Vec::new(),
        render_timer: timer.handle(),
        #[cfg(feature = "debug")]
        fps_texture,
        logger: log.clone(),
    };
    let mut state = Cum{
        display: display.clone(),
        event_loop: event_loop.handle(),
        space: Rc::new(RefCell::new(Space::new(None))),
        data: CumBackend{
            winit_backend: None,
            udev_backend: Some(Rc::new(RefCell::new(data))),
            seat_name: Some(seat_name),
        },
        start_time: std::time::Instant::now(),
        pointer_location: Point::from((0.0, 0.0)),
        cursor_status: Mutex::new(CursorImageStatus::Default),
    };

    // re-render timer
    event_loop
        .handle()
        .insert_source(timer, |(dev_id, crtc), _, state| {
            state.clone().render(dev_id, Some(crtc))
        })
        .unwrap();

    /*
     * Initialize the udev backend
     */
    let udev_backend = match UdevBackend::new(&state.clone().data.seat_name.clone().unwrap(), log.clone()) {
        Ok(ret) => ret,
        Err(err) => {
            println!("Failed to initialize udev backend: {}", err);
            return;
        }
    };

    /*
     * Initialize a fake output (we render one screen to every device in this example)
     */

    /*
     * Initialize libinput backend
     */
    let mut libinput_context = Libinput::new_with_udev::<LibinputSessionInterface<AutoSession>>(
        (*state.clone().data.udev_backend.unwrap().borrow_mut()).session.clone().into(),
    );
    libinput_context.udev_assign_seat(&state.clone().data.seat_name.clone().unwrap()).unwrap();
    let mut libinput_backend = LibinputInputBackend::new(libinput_context, log.clone());
    libinput_backend.link(session_signal);

    /*
     * Bind all our objects that get driven by the event loop
     */
    event_loop
        .handle()
        .insert_source(libinput_backend, move |event, _, state| {
            //state.process_input_event(event)
        })
        .unwrap();
    event_loop
        .handle()
        .insert_source(notifier, |(), &mut (), _state| {})
        .unwrap();
    for (dev, path) in udev_backend.device_list() {
        state.clone().device_added(dev, path.into())
    }

    {
        init_dmabuf_global(
            &mut *display.borrow_mut(),
            dmabuf_formats,
            |buffer, mut ddata| {
                let mut state = ddata.get::<Cum>().unwrap().clone();
                let mut back = state.data.udev_backend.unwrap();
                back.clone().borrow_mut()
                    .gpus
                    .renderer::<Gles2Renderbuffer>(
                        &back.clone().borrow_mut().primary_gpu,
                        &back.clone().borrow_mut().primary_gpu,
                    )
                    .unwrap()
                    .import_dmabuf(buffer, None)
                    .is_ok()
            },
            log.clone(),
        );
    }

    event_loop
        .handle()
        .insert_source(udev_backend, move |event, _, state| match event {
            UdevEvent::Added { device_id, path } => state.device_added(device_id, path),
            UdevEvent::Changed { device_id } => state.device_changed(device_id),
            UdevEvent::Removed { device_id } => state.device_removed(device_id),
        })
        .unwrap();

    /*
     * Start XWayland if supported
     */
    #[cfg(feature = "xwayland")]
    state.start_xwayland();

    /*
     * And run our loop
     */

    loop {
        if event_loop
            .dispatch(Some(Duration::from_millis(16)), &mut state)
            .is_err()
        {
            //state.running.store(false, Ordering::SeqCst);
        } else {
            state.space.borrow_mut().refresh();
           // state.popups.borrow_mut().cleanup();
            display.borrow_mut().flush_clients(&mut state);
        }
    }
}

pub type RenderSurface = GbmBufferedSurface<Rc<RefCell<GbmDevice<SessionFd>>>, SessionFd>;

struct SurfaceData {
    device_id: DrmNode,
    render_node: DrmNode,
    surface: RenderSurface,
    global: Option<Global<wl_output::WlOutput>>,
    #[cfg(feature = "debug")]
    fps: fps_ticker::Fps,
}

impl Drop for SurfaceData {
    fn drop(&mut self) {
        if let Some(global) = self.global.take() {
            global.destroy();
        }
    }
}

struct BackendData {
    _restart_token: SignalToken,
    surfaces: Rc<RefCell<HashMap<crtc::Handle, Rc<RefCell<SurfaceData>>>>>,
    gbm: Rc<RefCell<GbmDevice<SessionFd>>>,
    registration_token: RegistrationToken,
    event_dispatcher: Dispatcher<'static, DrmDevice<SessionFd>, Cum>,
}

fn scan_connectors(
    device_id: DrmNode,
    device: &DrmDevice<SessionFd>,
    gbm: &Rc<RefCell<GbmDevice<SessionFd>>>,
    display: &mut Display,
    space: &mut Space,
    signaler: &Signaler<SessionSignal>,
    logger: Option<u8>,
) -> HashMap<crtc::Handle, Rc<RefCell<SurfaceData>>> {
    // Get a set of all modesetting resource handles (excluding planes):
    let res_handles = device.resource_handles().unwrap();

    // Find all connected output ports.
    let connector_infos: Vec<ConnectorInfo> = res_handles
        .connectors()
        .iter()
        .map(|conn| device.get_connector(*conn).unwrap())
        .filter(|conn| conn.state() == ConnectorState::Connected)
        .inspect(|conn| println!("Connected: {:?}", conn.interface()))
        .collect();

    let mut backends = HashMap::new();

    let (render_node, formats) = {
        let display = EGLDisplay::new(&*gbm.borrow_mut(), None).unwrap();
        let node = match EGLDevice::device_for_display(&display)
            .ok()
            .and_then(|x| x.try_get_render_node().ok().flatten())
        {
            Some(node) => node,
            None => return HashMap::new(),
        };
        let context = EGLContext::new(&display, None).unwrap();
        (node, context.dmabuf_render_formats().clone())
    };

    // very naive way of finding good crtc/encoder/connector combinations. This problem is np-complete
    for connector_info in connector_infos {
        let encoder_infos = connector_info
            .encoders()
            .iter()
            .flatten()
            .flat_map(|encoder_handle| device.get_encoder(*encoder_handle))
            .collect::<Vec<EncoderInfo>>();

        let crtcs = encoder_infos
            .iter()
            .flat_map(|encoder_info| res_handles.filter_crtcs(encoder_info.possible_crtcs()));

        for crtc in crtcs {
            // Skip CRTCs used by previous connectors.
            let entry = match backends.entry(crtc) {
                Entry::Vacant(entry) => entry,
                Entry::Occupied(_) => continue,
            };

            println!(
                "Trying to setup connector {:?}-{} with crtc {:?}",
                connector_info.interface(),
                connector_info.interface_id(),
                crtc,
            );

            let mode = connector_info.modes()[0];
            let mut surface = match device.create_surface(crtc, mode, &[connector_info.handle()]) {
                Ok(surface) => surface,
                Err(err) => {
                    println!("Failed to create drm surface: {:?}", err);
                    continue;
                }
            };
            surface.link(signaler.clone());

            let gbm_surface =
                match GbmBufferedSurface::new(surface, gbm.clone(), formats.clone(), None) {
                    Ok(renderer) => renderer,
                    Err(err) => {
                        println!("Failed to create rendering surface: {}", err);
                        continue;
                    }
                };

            let size = mode.size();
            let mode = Mode {
                size: (size.0 as i32, size.1 as i32).into(),
                refresh: mode.vrefresh() as i32 * 1000,
            };

            let interface_short_name = match connector_info.interface() {
                drm::control::connector::Interface::DVII => Cow::Borrowed("DVI-I"),
                drm::control::connector::Interface::DVID => Cow::Borrowed("DVI-D"),
                drm::control::connector::Interface::DVIA => Cow::Borrowed("DVI-A"),
                drm::control::connector::Interface::SVideo => Cow::Borrowed("S-VIDEO"),
                drm::control::connector::Interface::DisplayPort => Cow::Borrowed("DP"),
                drm::control::connector::Interface::HDMIA => Cow::Borrowed("HDMI-A"),
                drm::control::connector::Interface::HDMIB => Cow::Borrowed("HDMI-B"),
                drm::control::connector::Interface::EmbeddedDisplayPort => Cow::Borrowed("eDP"),
                other => Cow::Owned(format!("{:?}", other)),
            };

            let output_name = format!("{}-{}", interface_short_name, connector_info.interface_id());

            let (phys_w, phys_h) = connector_info.size().unwrap_or((0, 0));
            let output = Output::new(
                output_name,
                PhysicalProperties {
                    size: (phys_w as i32, phys_h as i32).into(),
                    subpixel: wl_output::Subpixel::Unknown,
                    make: "Smithay".into(),
                    model: "Generic DRM".into(),
                },
                None,
            );
            let global = output.create_global(display);
            let position = (
                space
                    .outputs()
                    .fold(0, |acc, o| acc + space.output_geometry(o).unwrap().size.w),
                0,
            )
                .into();
            output.change_current_state(Some(mode), None, None, Some(position));
            output.set_preferred(mode);
            space.map_output(&output, position);

            output
                .user_data()
                .insert_if_missing(|| UdevOutputId { crtc, device_id });

            entry.insert(Rc::new(RefCell::new(SurfaceData {
                device_id,
                render_node,
                surface: gbm_surface,
                global: Some(global),
                #[cfg(feature = "debug")]
                fps: fps_ticker::Fps::default(),
            })));

            break;
        }
    }

    backends
}

impl Cum {
    fn device_added(&mut self, device_id: dev_t, path: PathBuf) {
        // Try to open the device
        let open_flags = OFlag::O_RDWR | OFlag::O_CLOEXEC | OFlag::O_NOCTTY | OFlag::O_NONBLOCK;
        let device_fd = self.clone().data.udev_backend.unwrap().borrow_mut().session.open(&path, open_flags).ok();
        let devices = device_fd
            .map(SessionFd)
            .map(|fd| (DrmDevice::new(fd, true, None), GbmDevice::new(fd)));

        // Report device open failures.
        let (mut device, gbm) = match devices {
            Some((Ok(drm), Ok(gbm))) => (drm, gbm),
            Some((Err(err), _)) => {
                println!(
                    "Skipping device {:?}, because of drm error: {:?}", device_id, err
                );
                return;
            }
            Some((_, Err(err))) => {
                println!(
                    "Skipping device {:?}, because of gbm error: {:?}", device_id, err
                );
                return;
            }
            None => return,
        };

        let gbm = Rc::new(RefCell::new(gbm));
        let node = match DrmNode::from_dev_id(device_id) {
            Ok(node) => node,
            Err(err) => {
                println!("Failed to access drm node for {}: {:?}", device_id, err);
                return;
            }
        };
        let backends = Rc::new(RefCell::new(scan_connectors(
            node,
            &device,
            &gbm,
            &mut *self.display.borrow_mut(),
            &mut *self.space.borrow_mut(),
            &(*self.clone().data.udev_backend.unwrap().borrow_mut()).signaler,
            None,
        )));

        let handle = self.clone().event_loop.clone();
        let restart_token = self.clone().data.udev_backend.unwrap().borrow_mut().signaler.register(move |signal| match signal {
            SessionSignal::ActivateSession | SessionSignal::ActivateDevice { .. } => {
                handle.insert_idle(move |anvil_state| anvil_state.render(node, None));
            }
            _ => {}
        });

        device.link(self.clone().data.udev_backend.unwrap().borrow_mut().signaler.clone());
        let event_dispatcher =
            Dispatcher::new(
                device,
                move |event, _, anvil_state: &mut Cum| match event {
                    DrmEvent::VBlank(crtc) => anvil_state.render(node, Some(crtc)),
                    DrmEvent::Error(error) => {
                        println!("{:?}", error);
                    }
                },
            );
        let registration_token = self.event_loop.register_dispatcher(event_dispatcher.clone()).unwrap();

        for backend in backends.borrow_mut().values() {
            // render first frame
            println!("Scheduling frame");
            schedule_initial_render(
                &mut self.clone().data.udev_backend.unwrap().borrow_mut().gpus,
                backend.clone(),
                &self.event_loop,
                None,
            );
        }

        self.clone().data.udev_backend.unwrap().borrow_mut().backends.insert(
            node,
            BackendData {
                _restart_token: restart_token,
                registration_token,
                event_dispatcher,
                surfaces: backends,
                gbm,
            },
        );
    }

    fn device_changed(&mut self, device: dev_t) {
        let node = match DrmNode::from_dev_id(device).ok() {
            Some(node) => node,
            None => return, // we already logged a warning on device_added
        };

        //quick and dirty, just re-init all backends
        if let Some(ref mut backend_data) = self.clone().data.udev_backend.unwrap().borrow_mut().backends.get_mut(&node) {
            let loop_handle = self.clone().event_loop.clone();
            let signaler = self.clone().data.udev_backend.unwrap().borrow_mut().signaler.clone();
            let mut space = self.space.borrow_mut();

            // scan_connectors will recreate the outputs (and sadly also reset the scales)
            for output in space
                .outputs()
                .filter(|o| {
                    o.user_data()
                        .get::<UdevOutputId>()
                        .map(|id| id.device_id == node)
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
            {
                space.unmap_output(&output);
            }

            let source = backend_data.event_dispatcher.as_source_mut();
            let mut backends = backend_data.surfaces.borrow_mut();
            *backends = scan_connectors(
                node,
                &source,
                &backend_data.gbm,
                &mut *self.display.borrow_mut(),
                &mut *space,
                &signaler,
                None,
            );

            // fixup window coordinates
            crate::helpers::fixup_positions(&mut *space);

            for surface in backends.values() {
                // render first frame
                schedule_initial_render(&mut self.clone().data.udev_backend.unwrap().borrow_mut().gpus, surface.clone(), &loop_handle, None);
            }
        }
    }

    fn device_removed(&mut self, device: dev_t) {
        let node = match DrmNode::from_dev_id(device).ok() {
            Some(node) => node,
            None => return, // we already logged a warning on device_added
        };
        // drop the backends on this side
        if let Some(backend_data) = self.clone().data.udev_backend.unwrap().borrow_mut().backends.remove(&node) {
            // drop surfaces
            backend_data.surfaces.borrow_mut().clear();
            println!("Surfaces dropped");
            let mut space = self.space.borrow_mut();

            for output in space
                .outputs()
                .filter(|o| {
                    o.user_data()
                        .get::<UdevOutputId>()
                        .map(|id| id.device_id == node)
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
            {
                space.unmap_output(&output);
            }
            crate::helpers::fixup_positions(&mut *space);

            let _device = self.event_loop.remove(backend_data.registration_token);
            let _device = backend_data.event_dispatcher.into_source_inner();

            println!("Dropping device");
        }
    }

    // If crtc is `Some()`, render it, else render all crtcs
    fn render(&mut self, dev_id: DrmNode, crtc: Option<crtc::Handle>) {
        let data = self.clone().data.udev_backend.unwrap();
        let mut unwrapped_data = data.borrow_mut();
        let device_backend = match unwrapped_data.backends.get_mut(&dev_id) {
            Some(backend) => backend,
            None => {
                println!("Trying to render on non-existent backend {}", dev_id);
                return;
            }
        };
        // setup two iterators on the stack, one over all surfaces for this backend, and
        // one containing only the one given as argument.
        // They make a trait-object to dynamically choose between the two
        let surfaces = device_backend.surfaces.borrow_mut();
        let mut surfaces_iter = surfaces.iter();
        let mut option_iter = crtc
            .iter()
            .flat_map(|crtc| surfaces.get(crtc).map(|surface| (crtc, surface)));

        let to_render_iter: &mut dyn Iterator<Item = (&crtc::Handle, &Rc<RefCell<SurfaceData>>)> =
            if crtc.is_some() {
                &mut option_iter
            } else {
                &mut surfaces_iter
            };

        for (&crtc, surface) in to_render_iter {
            // TODO get scale from the rendersurface when supporting HiDPI
            let frame = self.clone()
                .data.udev_backend.unwrap().borrow_mut()
                .pointer_image
                .get_image(1 /*scale*/, self.start_time.elapsed().as_millis() as u32);
            let primary_gpu = self.clone().data.udev_backend.unwrap().borrow_mut().primary_gpu;
            let mut tmp1 = self.clone().data.udev_backend.unwrap();
            let mut tmp = tmp1.borrow_mut();
            let mut renderer = tmp
                .gpus
                .renderer::<Gles2Renderbuffer>(&primary_gpu, &surface.borrow_mut().render_node)
                .unwrap();
            let pointer_images = &mut tmp1.borrow_mut().pointer_images;
            let pointer_image = pointer_images
                .iter()
                .find_map(|(image, texture)| if image == &frame { Some(texture) } else { None })
                .cloned()
                .unwrap_or_else(|| {
                    let texture = renderer
                        .import_memory(
                            &frame.pixels_rgba,
                            (frame.width as i32, frame.height as i32).into(),
                            false,
                        )
                        .expect("Failed to import cursor bitmap");
                    pointer_images.push((frame, texture.clone()));
                    texture
                });

            let result = render_surface(
                &mut *surface.borrow_mut(),
                &mut renderer,
                crtc,
                &mut *self.space.borrow_mut(),
                self.pointer_location,
                &pointer_image,
                &mut *self.cursor_status.lock().unwrap(),
                None,
            );
            let reschedule = match result {
                Ok(has_rendered) => !has_rendered,
                Err(err) => {
                    println!("Error during rendering: {:?}", err);
                    match err {
                        SwapBuffersError::AlreadySwapped => false,
                        SwapBuffersError::TemporaryFailure(err) => !matches!(
                            err.downcast_ref::<DrmError>(),
                            Some(&DrmError::DeviceInactive)
                                | Some(&DrmError::Access {
                                    source: drm::SystemError::PermissionDenied,
                                    ..
                                })
                        ),
                        SwapBuffersError::ContextLost(err) => panic!("Rendering loop lost: {}", err),
                    }
                }
            };

            if reschedule {
                self.clone().data.udev_backend.unwrap().borrow_mut().render_timer.add_timeout(
                    Duration::from_millis(1000 /*a seconds*/ / 60 /*refresh rate*/),
                    (dev_id, crtc),
                );
            }

            // Send frame events so that client start drawing their next frame
            self.space
                .borrow_mut()
                .send_frames(self.start_time.elapsed().as_millis() as u32);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_surface(
    surface: &mut SurfaceData,
    renderer: &mut UdevRenderer<'_>,
    crtc: crtc::Handle,
    space: &mut Space,
    pointer_location: Point<f64, Logical>,
    pointer_image: &MultiTexture,
    cursor_status: &mut CursorImageStatus,
    logger: Option<u8>,
) -> Result<bool, SwapBuffersError> {
    surface.surface.frame_submitted()?;

    let output = if let Some(output) = space.outputs().find(|o| {
        o.user_data().get::<UdevOutputId>()
            == Some(&UdevOutputId {
            device_id: surface.device_id,
            crtc,
        })
    }) {
        output.clone()
    } else {
        // somehow we got called with an invalid output
        return Ok(true);
    };
    let output_geometry = space.output_geometry(&output).unwrap();

    let (dmabuf, age) = surface.surface.next_buffer()?;
    renderer.bind(dmabuf)?;

    let mut elements: Vec<CustomElem> = Vec::new();
    // set cursor
    if output_geometry.to_f64().contains(pointer_location) {
        let (ptr_x, ptr_y) = pointer_location.into();
        let ptr_location = Point::<i32, Logical>::from((ptr_x as i32, ptr_y as i32)); // - output_geometry.loc;
        // draw the dnd icon if applicable
        /*{
            if let Some(ref wl_surface) = dnd_icon.as_ref() {
                if wl_surface.as_ref().is_alive() {
                    elements.push(draw_dnd_icon((*wl_surface).clone(), ptr_location, logger).into());
                }
            }
        }
         */

        // draw the cursor as relevant
        {
            // reset the cursor if the surface is no longer alive
            let mut reset = false;
            if let CursorImageStatus::Image(ref surface) = *cursor_status {
                reset = !surface.as_ref().is_alive();
            }
            if reset {
                *cursor_status = CursorImageStatus::Default;
            }

            if let CursorImageStatus::Image(ref wl_surface) = *cursor_status {
                elements.push(draw_cursor(wl_surface.clone(), ptr_location).into());
            } else {
                elements.push(PointerElement::new(pointer_image.clone(), ptr_location).into());
            }
        }
    }

    // and draw to our buffer
    // TODO we can pass the damage rectangles inside a AtomicCommitRequest
    let render_res = crate::render::render_output(&output, space, renderer, age.into(), &*elements)
        .map(|x| x.is_some());

    match render_res.map_err(|err| match err {
        RenderError::Rendering(err) => err.into(),
        _ => unreachable!(),
    }) {
        Ok(true) => {
            surface
                .surface
                .queue_buffer()
                .map_err(Into::<SwapBuffersError>::into)?;
            Ok(true)
        }
        x => x,
    }
}

fn schedule_initial_render(
    gpus: &mut GpuManager<EglGlesBackend>,
    surface: Rc<RefCell<SurfaceData>>,
    evt_handle: &LoopHandle<'static, Cum>,
    logger: Option<u8>,
) {
    let node = surface.borrow_mut().render_node;
    let result = {
        let mut renderer = gpus.renderer::<Gles2Renderbuffer>(&node, &node).unwrap();
        let mut surface = surface.borrow_mut();
        initial_render(&mut surface.surface, &mut renderer)
    };
    if let Err(err) = result {
        match err {
            SwapBuffersError::AlreadySwapped => {}
            SwapBuffersError::TemporaryFailure(err) => {
                // TODO dont reschedule after 3(?) retries
                println!("Failed to submit page_flip: {}", err);
                let handle = evt_handle.clone();
                evt_handle.insert_idle(move |data| {
                    schedule_initial_render(&mut (*data.data.udev_backend.clone().unwrap().borrow_mut()).gpus, surface, &handle, logger)
                });
            }
            SwapBuffersError::ContextLost(err) => panic!("Rendering loop lost: {}", err),
        }
    }
}

fn initial_render(
    surface: &mut RenderSurface,
    renderer: &mut UdevRenderer<'_>,
) -> Result<(), SwapBuffersError> {
    let (dmabuf, _age) = surface.next_buffer()?;
    renderer.bind(dmabuf)?;
    // Does not matter if we render an empty frame
    renderer
        .render((1, 1).into(), Transform::Normal, |_, frame| {
            frame
                .clear(
                    CLEAR_COLOR,
                    &[Rectangle::from_loc_and_size((0.0, 0.0), (1.0, 1.0))],
                )
                .map_err(Into::<SwapBuffersError>::into)
        })
        .map_err(Into::<SwapBuffersError>::into)
        .and_then(|x| x.map_err(Into::<SwapBuffersError>::into))?;
    surface.queue_buffer()?;
    surface.reset_buffers();
    Ok(())
}