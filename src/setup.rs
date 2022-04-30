use std::ffi::CStr;
use std::num::NonZeroU32;
use std::os::raw::{c_char, c_int, c_long, c_uint, c_ulong};
use std::ptr;
use std::ptr::{null, null_mut};
use libsex::bindings::{_XImage_funcs, AllocNone, CompositeRedirectManual, CopyFromParent, CWColormap, CWEventMask, Display, ExposureMask, GL_FALSE, GLfloat, glViewport, GLX_BIND_TO_TEXTURE_RGB_EXT, GLX_BIND_TO_TEXTURE_RGBA_EXT, GLX_BIND_TO_TEXTURE_TARGETS_EXT, GLX_DEPTH_SIZE, GLX_DOUBLEBUFFER, GLX_DRAWABLE_TYPE, GLX_NONE, GLX_PIXMAP_BIT, GLX_RED_SIZE, GLX_RGBA, GLX_TEXTURE_2D_BIT_EXT, GLX_Y_INVERTED_EXT, glXChooseVisual, GLXContext, glXCreateContext, glXGetFBConfigAttrib, glXGetFBConfigs, glXGetVisualFromFBConfig, glXMakeCurrent, InputOutput, LSBFirst, PictOpSrc, Screen, ShapeBounding, ShapeInput, Visual, Window, XCompositeGetOverlayWindow, XCompositeQueryExtension, XCompositeRedirectSubwindows, XCreateColormap, XCreatePixmap, XCreateWindow, XDefaultRootWindow, XFixed, XFixesCreateRegion, XFixesDestroyRegion, XFixesSetWindowShapeRegion, XGetErrorText, XImage, XInitImage, XMapWindow, XOpenDisplay, XPutImage, XRenderComposite, XRenderCreatePicture, XRenderFindVisualFormat, XRenderSetPictureTransform, XReparentWindow, XRootWindow, XScreenNumberOfScreen, XSetErrorHandler, XSetWindowAttributes, XTransform, XVisualIDFromVisual, XVisualInfo, ZPixmap};
use stb_image::image::LoadResult;
use crate::{allow_input_passthrough, fr, rgba_to_bgra};

pub fn setup_compositing(display: *mut Display, root: Window) -> (Window) {
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

    overlay_window
}

pub fn setup_desktop(display: *mut Display, screen: *mut Screen, visual: *mut Visual,root: Window,
                     src_width: u16, src_height: u16) -> Window{

    let desktop = unsafe { XCreateWindow(display,  root,
                                         0, 0,
                                         src_width as c_uint, src_height as c_uint,
                                         0, CopyFromParent as c_int, InputOutput, visual, 0, &mut XSetWindowAttributes {
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

    let mut bg_image_data = dst_image.buffer();

    // create a pixmap to draw on
    let pixmap = unsafe {
        XCreatePixmap(display, desktop,
                      src_width as c_uint, src_height as c_uint,
                      CopyFromParent as c_uint)
    };

    let mut image: XImage = XImage{
        width: src_width as c_int,
        height: src_height as c_int,
        xoffset: 0,
        format: ZPixmap as c_int,
        data: bg_image_data.as_mut_ptr() as *mut c_char,
        byte_order: LSBFirst as c_int,
        bitmap_unit: 32,
        bitmap_bit_order: LSBFirst as c_int,
        bitmap_pad: ZPixmap as c_int,
        depth: 24,
        bytes_per_line: 0,
        bits_per_pixel: 32,
        red_mask: 0, // suspicious
        green_mask: 0, // point of the mask
        blue_mask: 0, // i don't have a joke for this one
        obdata: null_mut(),
        f: _XImage_funcs {
            create_image: None,
            destroy_image: None,
            get_pixel: None,
            put_pixel: None,
            sub_image: None,
            add_pixel: None
        }
    };
    unsafe {
        XInitImage(&mut image);
    }

    // put the image on the pixmap
    unsafe {
        XPutImage(display, pixmap, (*screen).default_gc, &mut image, 0, 0, 0, 0, src_width as c_uint, src_height as c_uint);
    }

    // calculate the amount of times to multiply the image by
    let mut transform: XTransform = XTransform {
        matrix: [
            [1.0 as XFixed, 0.0 as XFixed, 0.0 as XFixed],
            [0.0 as XFixed, 1.0 as XFixed, 0.0 as XFixed],
            [0.0 as XFixed, 0.0 as XFixed, divide_factor as XFixed]
        ]
    };

    // get pictformat
    let pict_format = unsafe {
        XRenderFindVisualFormat(display, visual)
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

    desktop
}

unsafe extern "C" fn error_handler(display: *mut libsex::bindings::Display, error_event: *mut libsex::bindings::XErrorEvent) -> c_int {
    unsafe {
        let mut buffer: [c_char; 256] = [0; 256];
        XGetErrorText(display, (*error_event).error_code as c_int, buffer.as_mut_ptr(), 256);
        println!("{}", CStr::from_ptr(buffer.as_ptr()).to_str().unwrap());
    }
    0
}

pub unsafe fn setup_glx(overlay: Window, src_width: u32, src_height: u32, screen: *mut Screen)
    -> (GLXContext, *mut libsex::bindings::Display, *mut libsex::bindings::XVisualInfo, libsex::bindings::GLXFBConfig,
    c_int) {
    XSetErrorHandler(Some(error_handler));
    let display = XOpenDisplay(ptr::null());
    if display.is_null() {
        panic!("Could not open display");
    }
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

    (ctx, display, visinfo, *fbconfigs.offset(wanted_config as isize), value)
}