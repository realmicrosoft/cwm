use std::ffi::{c_void, CStr};
use std::num::NonZeroU32;
use std::os::raw::{c_char, c_int, c_long, c_uint, c_ulong};
use std::{mem, ptr};
use std::ptr::{null, null_mut};
use libsex::bindings::{_XImage_funcs, AllocNone, CompositeRedirectManual, CopyFromParent, CWColormap, CWEventMask, Display, ExposureMask, GC, GL_FALSE, GLfloat, glViewport, GLX_BIND_TO_TEXTURE_RGB_EXT, GLX_BIND_TO_TEXTURE_RGBA_EXT, GLX_BIND_TO_TEXTURE_TARGETS_EXT, GLX_DEPTH_SIZE, GLX_DOUBLEBUFFER, GLX_DRAWABLE_TYPE, GLX_NONE, GLX_PIXMAP_BIT, GLX_RED_SIZE, GLX_RGBA, GLX_TEXTURE_2D_BIT_EXT, GLX_Y_INVERTED_EXT, glXChooseVisual, GLXContext, glXCreateContext, glXGetFBConfigAttrib, glXGetFBConfigs, glXGetVisualFromFBConfig, glXMakeCurrent, InputOutput, LSBFirst, PictFormat, PictOpSrc, Screen, ShapeBounding, ShapeInput, Visual, Window, X_RenderQueryPictFormats, XCompositeGetOverlayWindow, XCompositeQueryExtension, XCompositeRedirectSubwindows, XCopyPlane, XCreateBitmapFromData, XCreateColormap, XCreateGC, XCreateImage, XCreatePixmap, XCreateWindow, XDefaultRootWindow, XDestroyWindow, XFixed, XFixesCreateRegion, XFixesDestroyRegion, XFixesSetWindowShapeRegion, XFree, XFreePixmap, XGetErrorText, XImage, XInitImage, XMapWindow, XOpenDisplay, XPutImage, XRenderComposite, XRenderCreatePicture, XRenderDirectFormat, XRenderFindVisualFormat, XRenderPictFormat, XRenderSetPictureTransform, XReparentWindow, XRootWindow, XScreenNumberOfScreen, XSetErrorHandler, XSetWindowAttributes, XSync, XTransform, XVisualIDFromVisual, XVisualInfo, ZPixmap};
use stb_image::image::LoadResult;
use crate::{allow_input_passthrough, fr, rgba_to_bgra};

pub fn setup_compositing(display: *mut Display, root: Window) -> (Window, GC) {
    // redirect subwindows of root window
    unsafe {
        XCompositeRedirectSubwindows(display, root, CompositeRedirectManual as c_int);
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
        XCreateGC(display, overlay_window, 0, null_mut())
    };

    (overlay_window, gc)
}

pub fn setup_desktop(display: *mut Display, gc: GC, visual: *mut XVisualInfo, pict_format: *mut XRenderPictFormat, root: Window,
                     src_width: u16, src_height: u16) -> Window{

    let desktop = unsafe { XCreateWindow(display,  root,
                                         0, 0,
                                         src_width as c_uint, src_height as c_uint,
                                         0, CopyFromParent as c_int, InputOutput, (*visual).visual, 0, &mut XSetWindowAttributes {
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
            event_mask: ExposureMask as c_long,
            do_not_propagate_mask: 0,
            override_redirect: 0,
            colormap: 0,
            cursor: 0
        }) };

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
                      CopyFromParent as c_uint)
    };

    let mut img: *mut XImage = unsafe { mem::zeroed() };

    let data = rgba_to_bgra(&bg_image_vec).as_mut_ptr() as *mut c_char;

    img = unsafe {
        XCreateImage(display, (*visual).visual, 24, ZPixmap as c_int, 0,
                     data,
                     bg_image_width.get(), bg_image_height.get(), 32, 0)
    };

    unsafe {
        XSync(display, 0);
    }

    unsafe {
        XInitImage(img);
    }

    // put the image on the pixmap
    unsafe {
        XPutImage(display, pixmap, gc, img, 0, 0, 0, 0,
                  bg_image_width.get() as c_uint, bg_image_height.get() as c_uint);
    }

    // calculate the amount of times to multiply the image by
    let mut transform: XTransform = XTransform {
        matrix: [
            [1.0 as XFixed, 0.0 as XFixed, 0.0 as XFixed],
            [0.0 as XFixed, 1.0 as XFixed, 0.0 as XFixed],
            [0.0 as XFixed, 0.0 as XFixed, divide_factor as XFixed]
        ]
    };

    // create picture from pixmap
    let picture = unsafe {
        XRenderCreatePicture(display, pixmap, pict_format, 0, null_mut())
    };

    // set picture transform
    unsafe {
        XRenderSetPictureTransform(display, picture, &mut transform);
    }

    // get picture of desktop
    let picture_desktop = unsafe {
        XRenderCreatePicture(display, desktop, pict_format, 0, null_mut())
    };

    // copy picture to desktop
    unsafe {
        XRenderComposite(display, PictOpSrc as c_int, picture, 0, picture_desktop,
                         0, 0, 0, 0, 0, 0, src_width as c_uint, src_height as c_uint);
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

unsafe fn XDestroyImage(p0: *mut XImage) {
    if p0.is_null() {
        return;
    }
    if !(*p0).data.is_null() {
        XFree((*p0).data as *mut c_void);
    }
    if !(*p0).obdata.is_null() {
        XFree((*p0).obdata as *mut c_void);
    }
    XFree(p0 as *mut c_void);
}


pub unsafe fn setup_glx(display: *mut Display, overlay: Window, src_width: u32, src_height: u32, screen: *mut Screen)
    -> (GLXContext, *mut libsex::bindings::XVisualInfo, libsex::bindings::GLXFBConfig,
    c_int, *mut XRenderPictFormat) {
    let visual = glXChooseVisual(display, 0, [
        GLX_RGBA, GLX_DEPTH_SIZE, 24, GLX_DOUBLEBUFFER, GLX_NONE,
    ].as_mut_ptr() as *mut c_int);
    if visual.is_null() {
        panic!("Could not choose visual");
    }
    let ctx = glXCreateContext(display, visual, ptr::null_mut(), 1);
    if ctx.is_null() {
        panic!("Could not create context");
    }

    glXMakeCurrent(display, overlay, ctx);
    glViewport(0, 0, src_width as i32, src_height as i32);

    let mut nfbconfigs = 10;
    let fbconfigs = glXGetFBConfigs(display, XScreenNumberOfScreen(screen), &mut nfbconfigs);
    if fbconfigs.is_null() {
        panic!("Could not get fbconfigs");
    }
    let visualid = XVisualIDFromVisual((*visual).visual);
    let mut visinfo: *mut XVisualInfo = null_mut();
    let mut wanted_config = 0;
    let mut value: c_int = 0;
    for i in 0..nfbconfigs {
        visinfo = glXGetVisualFromFBConfig (display, *fbconfigs.offset(i as isize));
        if visinfo.is_null() || (*visinfo).visualid != visualid as u64 {
            continue;
        }

        glXGetFBConfigAttrib (display, *fbconfigs.offset(i as isize), GLX_DRAWABLE_TYPE as c_int, &mut value);
        if (value & GLX_PIXMAP_BIT as i32) != 1 {
            continue;
        }

        glXGetFBConfigAttrib (display, *fbconfigs.offset(i as isize),
                              GLX_BIND_TO_TEXTURE_TARGETS_EXT as c_int,
                              &mut value);
        if (value & GLX_TEXTURE_2D_BIT_EXT as i32) != 1 {
            continue;
        }

        glXGetFBConfigAttrib (display, *fbconfigs.offset(i as isize),
                              GLX_BIND_TO_TEXTURE_RGBA_EXT as c_int,
                              &mut value);
        if value == 0
        {
            glXGetFBConfigAttrib (display, *fbconfigs.offset(i as isize),
                                  GLX_BIND_TO_TEXTURE_RGB_EXT as c_int,
                                  &mut value);
            if value == 0 {
                continue;
            }
        }

        glXGetFBConfigAttrib (display, *fbconfigs.offset(i as isize),
                              GLX_Y_INVERTED_EXT as c_int,
                              &mut value);

        wanted_config = i;
        break;
    }
    println!("wanted config: {}", wanted_config);

    // get pict format
    let pict_format = unsafe {
        XRenderFindVisualFormat(display, (*visual).visual)
    };

    (ctx, visinfo, *fbconfigs.offset(wanted_config as isize), value, pict_format)
}