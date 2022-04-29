use std::{cell::RefCell, rc::Rc, sync::atomic::Ordering, time::Duration};
use std::borrow::Borrow;
use std::sync::Mutex;

use slog::Logger;
use smithay::{
    backend::renderer::{ImportDma, ImportEgl},
    wayland::dmabuf::init_dmabuf_global,
};
use smithay::{
    backend::{
        renderer::gles2::Gles2Renderer,
        winit::{self, WinitEvent},
        SwapBuffersError,
    },
    desktop::space::RenderError,
    reexports::{
        calloop::EventLoop,
        wayland_server::{
            protocol::{wl_output, wl_surface},
            Display,
        },
    },
    wayland::{
        output::{Mode, Output, PhysicalProperties},
        seat::CursorImageStatus,
    },
};
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::desktop::Space;
use smithay::utils::Point;

use crate::{
    drawing::*,
    Cum,
    CumBackend
};

pub const OUTPUT_NAME: &str = "winit";

#[derive(Clone)]
pub struct WinitData {
    full_redraw: u8,
}

pub fn run_winit() {
    let mut event_loop = EventLoop::try_new().unwrap();
    let display = Rc::new(RefCell::new(Display::new()));

    let (backend, mut winit) = match winit::init(None) {
        Ok(ret) => ret,
        Err(err) => {
            println!("Failed to initialize Winit backend: {:?}", err);
            return;
        }
    };
    let mut backend = Rc::new(RefCell::new(backend));

    if (backend.clone().borrow_mut())
        .renderer()
        .bind_wl_display(&display.borrow_mut())
        .is_ok()
    {
        println!("EGL hardware-acceleration enabled");
        let dmabuf_formats = backend
            .borrow_mut()
            .renderer()
            .dmabuf_formats()
            .cloned()
            .collect::<Vec<_>>();
        let backend = backend.clone();
        init_dmabuf_global(
            &mut display.borrow_mut(),
            dmabuf_formats,
            move |buffer, _| {
                backend
                    .borrow_mut()
                    .renderer()
                    .import_dmabuf(buffer, None)
                    .is_ok()
            },
            None,
        );
    };

    let size = backend.borrow_mut().window_size().physical_size;

    /*
     * Initialize the globals
     */

    let data = WinitData {
        full_redraw: 0,
    };
    let mut state = Cum{
        display: display.clone(),
        event_loop: event_loop.handle(),
        space: Rc::new(RefCell::new(Space::new(None))),
        data: CumBackend{
            winit_backend: Some(data),
            udev_backend: None,
            seat_name: None,
        },
        start_time: std::time::Instant::now(),
        pointer_location: Point::from((0.0, 0.0)),
        cursor_status: Mutex::new(CursorImageStatus::Default),
    };

    let mode = Mode {
        size,
        refresh: 60_000,
    };

    let output = Output::new(
        OUTPUT_NAME.to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: wl_output::Subpixel::Unknown,
            make: "CHAOTIC WINDOW MANAGER".into(),
            model: "Winit".into(),
        },
        None,
    );
    let _global = output.create_global(&mut *display.borrow_mut());
    output.change_current_state(
        Some(mode),
        Some(wl_output::Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.borrow_mut().map_output(&output, (0, 0));

    let start_time = std::time::Instant::now();

    println!("Initialization completed, starting the main loop.");

    loop {
        if winit
            .dispatch_new_events(|event| match event {
                WinitEvent::Resized { size, .. } => {
                    let mut space = state.space.borrow_mut();
                    // We only have one output
                    let output = space.outputs().next().unwrap().clone();
                    space.map_output(&output, (0, 0));
                    let mode = Mode {
                        size,
                        refresh: 60_000,
                    };
                    output.change_current_state(Some(mode), None, None, None);
                    output.set_preferred(mode);
                    crate::helpers::fixup_positions(&mut *space);
                }

                WinitEvent::Input(event) => state.process_input_event_windowed(event, OUTPUT_NAME),

                _ => (),
            })
            .is_err()
        {
            //state.running.store(false, Ordering::SeqCst);
            break;
        }

        // drawing logic
        {
            let mut backend = backend.borrow_mut();
            let mut cursor_visible: bool = false;

            let mut elements = Vec::<CustomElem<Gles2Renderer>>::new();
            let mut cursor_guard = state.cursor_status.lock().unwrap();
/*
            // draw the dnd icon if any
            if let Some(ref surface) = *dnd_guard {
                if surface.as_ref().is_alive() {
                    elements.push(
                        draw_dnd_icon(surface.clone(), state.pointer_location.to_i32_round(), &log).into(),
                    );
                }
            }

 */

            // draw the cursor as relevant
            // reset the cursor if the surface is no longer alive
            let mut reset = false;
            if let CursorImageStatus::Image(ref surface) = *cursor_guard {
                reset = !surface.as_ref().is_alive();
            }
            if reset {
                *cursor_guard = CursorImageStatus::Default;
            }
            if let CursorImageStatus::Image(ref surface) = *cursor_guard {
                cursor_visible = false;
                elements
                    .push(draw_cursor(surface.clone(), state.pointer_location.to_i32_round()).into());
            } else {
                cursor_visible = true;
            }
            let full_redraw = &mut (state.data.winit_backend.as_mut()).unwrap().full_redraw;
            *full_redraw = full_redraw.saturating_sub(1);
            let age = if *full_redraw > 0 {
                0
            } else {
                backend.buffer_age().unwrap_or(0)
            };
            let render_res = backend.bind().and_then(|_| {
                let renderer = backend.renderer();
                crate::render::render_output(
                    &output,
                    &mut *state.space.borrow_mut(),
                    renderer,
                    age,
                    &*elements,
                )
                    .map_err(|err| match err {
                        RenderError::Rendering(err) => err.into(),
                        _ => unreachable!(),
                    })
            });

            match render_res {
                Ok(Some(damage)) => {
                    let scale = output.current_scale().fractional_scale();
                    if let Err(err) = backend.submit(if age == 0 { None } else { Some(&*damage) }, scale) {
                        println!("Failed to submit buffer: {:?}", err);
                    }
                    //backend.window().set_cursor_visible(cursor_visible);
                }
                Ok(None) => backend.window().set_cursor_visible(cursor_visible),
                Err(SwapBuffersError::ContextLost(err)) => {
                    println!("Critical Rendering Error: {}", err);
                    //state.running.store(false, Ordering::SeqCst);
                }
                Err(err) => println!("Rendering error: {:?}", err),
            }
        }

        // Send frame events so that client start drawing their next frame
        state
            .space
            .borrow_mut()
            .send_frames(start_time.elapsed().as_millis() as u32);

        if event_loop
            .dispatch(Some(Duration::from_millis(16)), &mut state)
            .is_err()
        {
            //state.running.store(false, Ordering::SeqCst);
        } else {
            state.space.borrow_mut().refresh();
            //state.popups.borrow_mut().cleanup();
            display.borrow_mut().flush_clients(&mut state);
        }
    }
}