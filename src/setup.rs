use std::ffi::{c_void, CStr};
use std::num::NonZeroU32;
use std::os::raw::{c_char, c_int, c_long, c_uint, c_ulong};
use std::{mem, ptr};
use std::ptr::{null, null_mut};
use libsex::bindings::{_XImage_funcs, _XTransform, AllocNone, CompositeRedirectAutomatic, CompositeRedirectManual, CopyFromParent, CPSubwindowMode, CWColormap, CWEventMask, Display, ExposureMask, GC, GL_FALSE, GLbyte, GLfloat, GLubyte, glViewport, GLX_BIND_TO_TEXTURE_RGB_EXT, GLX_BIND_TO_TEXTURE_RGBA_EXT, GLX_BIND_TO_TEXTURE_TARGETS_EXT, GLX_DEPTH_SIZE, GLX_DOUBLEBUFFER, GLX_DRAWABLE_TYPE, GLX_NONE, GLX_PIXMAP_BIT, GLX_RED_SIZE, GLX_RGBA, GLX_TEXTURE_2D_BIT_EXT, GLX_Y_INVERTED_EXT, glXChooseVisual, GLXContext, glXCreateContext, GLXDrawable, glXGetFBConfigAttrib, glXGetFBConfigs, glXGetProcAddress, glXGetProcAddressARB, glXGetVisualFromFBConfig, glXMakeCurrent, IncludeInferiors, InputOutput, LSBFirst, PictFormat, PictOpSrc, PropertyChangeMask, Screen, ShapeBounding, ShapeInput, StructureNotifyMask, SubstructureNotifyMask, SubstructureRedirectMask, Visual, VisualNoMask, Window, X_RenderQueryPictFormats, XChangeWindowAttributes, XCompositeGetOverlayWindow, XCompositeQueryExtension, XCompositeRedirectSubwindows, XCopyPlane, XCreateBitmapFromData, XCreateColormap, XCreateGC, XCreateImage, XCreatePixmap, XCreateWindow, XDefaultDepth, XDefaultDepthOfScreen, XDefaultRootWindow, XDefaultVisual, XDefaultVisualOfScreen, XDestroyWindow, XFixed, XFixesCreateRegion, XFixesDestroyRegion, XFixesSetWindowShapeRegion, XFlush, XFree, XFreePixmap, XGetErrorText, XGetVisualInfo, XImage, XInitImage, XMapWindow, XOpenDisplay, XPutImage, XRenderComposite, XRenderCreatePicture, XRenderDirectFormat, XRenderFindVisualFormat, XRenderPictFormat, XRenderPictureAttributes, XRenderSetPictureTransform, XReparentWindow, XRootWindow, XScreenNumberOfScreen, XSelectInput, XSetErrorHandler, XSetWindowAttributes, XSync, XTransform, XVisualIDFromVisual, XVisualInfo, ZPixmap};
use stb_image::image::LoadResult;
use crate::{allow_input_passthrough, fr, get_window_fb_config, rgba_to_bgra};

pub fn setup_compositing(display: *mut Display, root: Window) -> (Window, GC) {
    let mut major = 0;
    let mut minor = 2;
    unsafe {
        let exist = XCompositeQueryExtension(display, &mut major, &mut minor);
        if exist == 0 {
            panic!("Compositing extension not found");
        }
    }
    // redirect subwindows of root window
    unsafe {
        XCompositeRedirectSubwindows(display, root, CompositeRedirectManual as c_int);
    }
    // enable events
    unsafe {
        XSelectInput(display, root, (SubstructureNotifyMask) as c_long);
    }

    // enable bigreq extension todo: check if this is needed

   // get overlay window
    let mut overlay_window: Window = 0;
    unsafe {
        overlay_window = XCompositeGetOverlayWindow(display, root);
    }
    if overlay_window == 0 {
        panic!("Could not get overlay window");
    }
    allow_input_passthrough(display, overlay_window, 0, 0);

    let gc = unsafe {
        XCreateGC(display, root, 0, null_mut())
    };


    (overlay_window, gc)
}

pub fn setup_desktop(display: *mut Display, gc: GC, screen: *mut Screen, pict_format: *mut XRenderPictFormat, root: Window,
                     src_width: u16, src_height: u16) -> Window{

    let desktop = unsafe { XCreateWindow(display, root,
                                         0, 0,
                                         src_width as c_uint, src_height as c_uint,
                                         0, XDefaultDepthOfScreen(screen), InputOutput, XDefaultVisualOfScreen(screen), CWEventMask as c_ulong, &mut XSetWindowAttributes {
            background_pixmap: 0,
            background_pixel: 0,
            border_pixmap: 0,
            border_pixel: 0,
            bit_gravity: 0,
            win_gravity: 0,
            backing_store: 0,
            backing_planes: 0,
            backing_pixel: 0,
            save_under: 0,
            event_mask: SubstructureNotifyMask as c_long,
            do_not_propagate_mask: 0,
            override_redirect: 0,
            colormap: 0,
            cursor: 0
        }) };
    unsafe {
        XSync(display, 0);
    }

    // load the bg.png image
    let bg_image_load = stb_image::image::load("bg.png");
    let bg_image = match bg_image_load {
        LoadResult::ImageU8(image) => image,
        LoadResult::ImageF32(_) => panic!("bg.png is not 8-bit"),
        LoadResult::Error(e) => panic!("Error loading bg.png: {}", e),
    };

    let bg_image_width = NonZeroU32::new(bg_image.width as u32).unwrap();
    let bg_image_height = NonZeroU32::new(bg_image.height as u32).unwrap();

    let mut divide_factor = 1;
    let mut potential_size: u32 = (src_width / divide_factor) as u32 * (src_height / divide_factor) as u32;
    // calculate the amount of bytes that src_width * src_height * 4 will take
    while potential_size > 300000 {
        divide_factor += 1;
        potential_size = (src_width / divide_factor) as u32 * (src_height / divide_factor) as u32;
    }

    let mut src = fr::Image::from_vec_u8(
        bg_image_width,
        bg_image_height,
        bg_image.data.clone(),
        fr::PixelType::U8x4,
    ).unwrap();
    // Create MulDiv instance
    let alpha_mul_div = fr::MulDiv::default();
    // Multiple RGB channels of source image by alpha channel
    alpha_mul_div
        .multiply_alpha_inplace(&mut src.view_mut())
        .unwrap();

    let dst_width = NonZeroU32::new((src_width / divide_factor) as u32).unwrap();
    let dst_height = NonZeroU32::new((src_height / divide_factor) as u32).unwrap();
    let mut dst_image = fr::Image::new(
        dst_width,
        dst_height,
        fr::PixelType::U8x4,
    );
    let mut dst_view = dst_image.view_mut();

    let mut resizer = fr::Resizer::new(
        fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3),
    );
    resizer.resize(&src.view(), &mut dst_view).unwrap();

    alpha_mul_div.divide_alpha_inplace(&mut dst_view).unwrap();

    let mut bg_image_buffer = dst_image.buffer();

    // create a vec from the buffer so we can make it mutable
    let mut bg_image_vec: Vec<u8> = bg_image_buffer.to_vec();

    // create a pixmap to draw on
    let mut pixmap = unsafe {
        XCreatePixmap(display, desktop,
                      src_width as c_uint, src_height as c_uint,
                      24 as c_uint)
    };

    unsafe {
        XSync(display, 0);
    }

    let mut img: *mut XImage = unsafe { mem::zeroed() };

    let mut data = rgba_to_bgra(&bg_image_vec);

    img = unsafe {
        XCreateImage(display, XDefaultVisualOfScreen(screen), 24, ZPixmap as c_int, 0,
                     data.as_mut_ptr() as *mut c_char,
                     dst_width.get(), dst_height.get(), 32, 0)
    };

    unsafe {
        XSync(display, 0);
    }


    unsafe {
        XInitImage(img);
    }
    unsafe {
        XSync(display, 0);
    }

    // put the image on the pixmap
    unsafe {
        XPutImage(display, pixmap, gc, img, 0, 0, 0, 0,
                  dst_width.get(), dst_height.get());
    }
    unsafe {
        XSync(display, 0);
    }

    // create picture from pixmap
    let picture = unsafe {
        XRenderCreatePicture(display, pixmap, XRenderFindVisualFormat(display, XDefaultVisualOfScreen(screen)), CPSubwindowMode as c_ulong, &XRenderPictureAttributes{
            repeat: 0,
            alpha_map: 0,
            alpha_x_origin: 0,
            alpha_y_origin: 0,
            clip_x_origin: 0,
            clip_y_origin: 0,
            clip_mask: 0,
            graphics_exposures: 0,
            subwindow_mode: IncludeInferiors as c_int,
            poly_edge: 0,
            poly_mode: 0,
            dither: 0,
            component_alpha: 0
        })
    };
    unsafe {
        XSync(display, 0);
    }

    // set picture transform
    unsafe {
        XRenderSetPictureTransform(display, picture, &mut XTransform {
            matrix: [
                [1.0 as XFixed, 0.0 as XFixed, 0.0 as XFixed],
                [0.0 as XFixed, 1.0 as XFixed, 0.0 as XFixed],
                [0.0 as XFixed, 0.0 as XFixed, divide_factor as XFixed]
            ]
        });
    }
    unsafe {
        XSync(display, 0);
    }


    // get picture of desktop
    let picture_desktop = unsafe {
        XRenderCreatePicture(display, desktop, pict_format, CPSubwindowMode as c_ulong, &XRenderPictureAttributes{
            repeat: 0,
            alpha_map: 0,
            alpha_x_origin: 0,
            alpha_y_origin: 0,
            clip_x_origin: 0,
            clip_y_origin: 0,
            clip_mask: 0,
            graphics_exposures: 0,
            subwindow_mode: IncludeInferiors as c_int,
            poly_edge: 0,
            poly_mode: 0,
            dither: 0,
            component_alpha: 0
        })
    };
    unsafe {
        XSync(display, 0);
    }

    // copy picture to desktop
    unsafe {
        XRenderComposite(display, PictOpSrc as c_int, picture, 0, picture_desktop,
                         0, 0, 0, 0, 0, 0, src_width as c_uint, src_height as c_uint);
    }
    unsafe {
        XSync(display, 0);
    }

    // map the window
    unsafe {
        XMapWindow(display, desktop);
    }

    // free the pixmap
    unsafe {
        XFreePixmap(display, pixmap);
    }

    desktop
}


pub unsafe fn setup_glx(display: *mut Display, overlay: Window, src_width: u32, src_height: u32, screen: *mut Screen)
    -> (GLXContext, *mut XVisualInfo, libsex::bindings::GLXFBConfig,
    c_int, *mut XRenderPictFormat){//}, (extern "C" fn(*mut Display, GLXDrawable, c_int, *mut c_int), extern "C" fn(*mut Display, GLXDrawable, c_int))) {
    let mut nfbconfigs = 10;
    /*
    let fbconfigs = glXGetFBConfigs(display, XScreenNumberOfScreen(screen), &mut nfbconfigs);
    if fbconfigs.is_null() {
        panic!("Could not get fbconfigs");
    }
    let visualid = (*XDefaultVisualOfScreen(screen)).visualid;
    let mut visinfo: *mut XVisualInfo = null_mut();
    let fbconfig = get_window_fb_config(overlay, display, screen);
    visinfo = glXGetVisualFromFBConfig (display, fbconfig);

     */
    let fbconfig = get_window_fb_config(overlay, display, screen);
    let visinfo = glXGetVisualFromFBConfig (display, fbconfig);
    // get pict format
    let pict_format = unsafe {
        XRenderFindVisualFormat(display, (*visinfo).visual)
    };


    let ctx = glXCreateContext(display, visinfo, null_mut(), 1);
    if ctx.is_null() {
        panic!("Could not create context");
    }

    glXMakeCurrent(display, overlay, ctx);
    glViewport(0, 0, src_width as i32, src_height as i32);

    // i'm crying
    /*let tex_from_img = unsafe {
        let proc_address = glXGetProcAddress(b"glXBindTexImageEXT\0".as_ptr() as *const GLubyte).unwrap() as *mut c_void;
        if proc_address.is_null() {
            panic!("glXBindTexImageEXT not found/supported");
        }
        let funny1 = mem::transmute::<*mut c_void, extern "C" fn(*mut Display, GLXDrawable, c_int, *mut c_int)>(proc_address as *mut c_void);
        let proc_address = glXGetProcAddress(b"glXReleaseTexImageEXT\0".as_ptr() as *const GLubyte).unwrap() as *mut c_void;
        if proc_address.is_null() {
            panic!("glXReleaseTexImageEXT not found/supported");
        }
        let funny2 = mem::transmute::<*mut c_void, extern "C" fn(*mut Display, GLXDrawable, c_int)>(proc_address as *mut c_void);
        (funny1, funny2)
    };
     */

    (ctx, visinfo, fbconfig, 0, pict_format)//, tex_from_img)
}