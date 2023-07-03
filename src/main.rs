use std::{
    cmp::Ordering,
    ffi::c_uchar,
    ops::Deref,
    ptr::{null, null_mut},
};

use xcb::Xid;

struct Monitor {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    window: xcb::x::Window,
    pixmap: xcb::x::Pixmap,
}

trait Rectangle {
    fn from_crtc(crtc: &xcb::randr::GetCrtcInfoReply) -> Self;
    fn is_inside(&self, rect: &Self) -> bool;
}

impl Rectangle for xcb::x::Rectangle {
    fn from_crtc(crtc: &xcb::randr::GetCrtcInfoReply) -> Self {
        Self {
            x: crtc.x(),
            y: crtc.y(),
            width: crtc.width(),
            height: crtc.height(),
        }
    }

    fn is_inside(&self, rect: &xcb::x::Rectangle) -> bool {
        let x = self.x >= rect.x && self.x + self.width as i16 <= rect.x + rect.width as i16;
        let y = self.y >= rect.y && self.y + self.height as i16 <= rect.y + rect.height as i16;
        x && y
    }
}

// Order rects from left to right, then from top to bottom.
// Edge cases for overlapping screens.
fn compare_rectangles(a: &xcb::x::Rectangle, b: &xcb::x::Rectangle) -> Ordering {
    if a.x != b.x {
        a.x.cmp(&b.x)
    } else {
        (a.y + a.height as i16).cmp(&b.y)
    }
}

struct XftDraw {
    draw: *mut x11::xft::XftDraw,
    color: x11::xft::XftColor,
    font: *mut x11::xft::XftFont,
}

impl XftDraw {
    fn draw_string(&mut self, text: &str) {
        unsafe {
            x11::xft::XftDrawString8(
                self.draw,
                &mut self.color as *mut x11::xft::XftColor,
                self.font,
                0,
                10,
                text.as_ptr() as *const c_uchar,
                text.len() as i32,
            )
        };
    }
}

impl Drop for XftDraw {
    fn drop(&mut self) {
        unsafe { x11::xft::XftDrawDestroy(self.draw) };
    }
}

struct Connection(xcb::Connection);

impl Deref for Connection {
    type Target = xcb::Connection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Connection {
    fn new() -> Self {
        let display = unsafe { x11::xlib::XOpenDisplay(null()) };

        let extensions = [xcb::Extension::RandR];
        let connection =
            unsafe { xcb::Connection::from_xlib_display_and_extensions(display, &extensions, &[]) };

        Self(connection)
    }

    /// Execute a request and wait for the reply. Check for request completion.
    fn exec<Request>(
        &self,
        request: &Request,
    ) -> <<Request as xcb::Request>::Cookie as xcb::CookieWithReplyChecked>::Reply
    where
        Request: xcb::Request,
        <Request as xcb::Request>::Cookie: xcb::CookieWithReplyChecked,
    {
        let cookie = self.send_request(request);
        self.wait_for_reply(cookie).unwrap()
    }

    /// Execute a request that has no reply. Check for request completion.
    fn exec_<Request>(&self, request: &Request)
    where
        Request: xcb::RequestWithoutReply + std::fmt::Debug,
    {
        if let Err(err) = self.send_and_check_request(request) {
            dbg!(&request);
            panic!("{}", err);
        };
    }
}

struct Setup {
    connection: Connection,
    root_window: xcb::x::Window,
    width: u16,
    height: u16,
    visual_id: u32,
    visual: *mut x11::xlib::Visual,
    colormap: xcb::x::Colormap,
}

impl Setup {
    fn new() -> Self {
        let connection = Connection::new();

        // How the layout looks like.
        let setup_info = connection.get_setup();
        assert_eq!(setup_info.roots().count(), 1);

        // The root screen - rendering canvas.
        let screen = setup_info.roots().nth(0).unwrap();

        // The root window, which is essentially a rect.
        let root_window = screen.root();
        let visual_id = screen
            .allowed_depths()
            .find_map(|depth| (depth.depth() == 32).then(|| depth.visuals()[0].visual_id()))
            .unwrap();

        let display = connection.get_raw_dpy();
        let mut visual_info_mask = x11::xlib::XVisualInfo {
            depth: 32,
            visual: null_mut(),
            visualid: 0, // TODO: Specify the id we got already?
            screen: 0,
            class: 0,
            red_mask: 0,
            green_mask: 0,
            blue_mask: 0,
            colormap_size: 0,
            bits_per_rgb: 0,
        };
        let mut result = 0;
        let visual_info = unsafe {
            x11::xlib::XGetVisualInfo(
                display,
                x11::xlib::VisualDepthMask,
                &mut visual_info_mask as *mut x11::xlib::XVisualInfo,
                &mut result as *mut i32,
            )
        };
        let visual_info = unsafe { *visual_info };
        assert_eq!(visual_info.visualid, visual_id as u64);
        let visual = visual_info.visual;

        let width = screen.width_in_pixels();
        let height = screen.height_in_pixels();

        let colormap: xcb::x::Colormap = connection.generate_id();
        connection.exec_(&xcb::x::CreateColormap {
            alloc: xcb::x::ColormapAlloc::None,
            mid: colormap,
            window: root_window,
            visual: visual_id,
        });

        Self {
            connection,
            root_window,
            width,
            height,
            visual_id,
            visual,
            colormap,
        }
    }

    fn get_screen_resources(&self) -> xcb::randr::GetScreenResourcesCurrentReply {
        self.connection
            .exec(&xcb::randr::GetScreenResourcesCurrent {
                window: self.root_window,
            })
    }

    fn get_crtc_info(&self, output: xcb::randr::Output) -> Option<xcb::randr::GetCrtcInfoReply> {
        let config_timestamp = xcb::x::CURRENT_TIME;
        let output_info = self.connection.exec(&xcb::randr::GetOutputInfo {
            output,
            config_timestamp,
        });

        let crtc = output_info.crtc();
        if crtc.is_none() || output_info.connection() != xcb::randr::Connection::Connected {
            // Something fishy, skup this output.
            return None;
        }

        Some(self.connection.exec(&xcb::randr::GetCrtcInfo {
            crtc,
            config_timestamp,
        }))
    }

    fn create_window_and_pixmap(
        &self,
        x: i16,
        y: i16,
        width: u16,
        height: u16,
        colormap: xcb::x::Colormap,
    ) -> (xcb::x::Window, xcb::x::Pixmap) {
        let window = self.connection.generate_id();
        let depth = 32; // TODO (visual == scr->root_visual) ? XCB_COPY_FROM_PARENT : 32;
        self.connection.exec_(&xcb::x::CreateWindow {
            depth,
            wid: window,
            parent: self.root_window,
            x,
            y,
            width,
            height,
            border_width: 0,
            class: xcb::x::WindowClass::InputOutput,
            visual: self.visual_id,
            value_list: &[
                xcb::x::Cw::BackPixel(0x00000000),
                xcb::x::Cw::BorderPixel(0x00000000),
                xcb::x::Cw::OverrideRedirect(false), // EMWH noncompliant
                xcb::x::Cw::EventMask(
                    xcb::x::EventMask::EXPOSURE | xcb::x::EventMask::BUTTON_PRESS,
                ),
                xcb::x::Cw::Colormap(colormap),
            ],
        });

        let pixmap = self.connection.generate_id();
        self.connection.exec_(&xcb::x::CreatePixmap {
            depth,
            pid: pixmap,
            drawable: xcb::x::Drawable::Window(window),
            width,
            height,
        });

        (window, pixmap)
    }

    fn create_gc(&self, drawable: xcb::x::Drawable, value_list: &[xcb::x::Gc]) -> xcb::x::Gcontext {
        let cid = self.connection.generate_id();
        self.connection.exec_(&xcb::x::CreateGc {
            cid,
            drawable,
            value_list,
        });
        cid
    }

    fn new_xft_draw(&self, pixmap: &xcb::x::Pixmap) -> XftDraw {
        let display = self.connection.get_raw_dpy();
        let visual = self.visual;
        let colormap = self.colormap.resource_id() as u64;

        let draw = unsafe {
            x11::xft::XftDrawCreate(display, pixmap.resource_id() as u64, visual, colormap)
        };
        if draw == null_mut() {
            panic!("Failed to create Xft draw.");
        }

        let mut render_color = x11::xrender::XRenderColor {
            red: 30000,
            green: 0,
            blue: 30000,
            alpha: 30000,
        };

        let mut color = x11::xft::XftColor {
            pixel: 0,
            color: x11::xrender::XRenderColor {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
        };

        let result = unsafe {
            x11::xft::XftColorAllocValue(
                display,
                visual,
                colormap,
                &mut render_color as *mut x11::xrender::XRenderColor,
                &mut color as *mut x11::xft::XftColor,
            )
        };
        dbg!(&color);
        if result == 0 {
            panic!("Failed to create Xft color.");
        }

        let font = b"UbuntuMono Nerd Font:size=12\0";
        let font = unsafe { x11::xft::XftFontOpenName(display, 0, font.as_ptr() as *const i8) };
        if result == 0 {
            panic!("Failed to create Xft font.");
        }

        XftDraw { draw, color, font }
    }
}

struct Bar {
    setup: Setup,
    monitors: Vec<Monitor>,
    bg_gc: xcb::x::Gcontext,
    fg_gc: xcb::x::Gcontext,
}

impl Bar {
    fn new() -> Self {
        let setup = Setup::new();

        let bar_height = 20;

        let screen_resources = setup.get_screen_resources();
        let outputs = screen_resources.outputs();

        // Get output regions.
        let mut regions = Vec::new();
        for output in outputs {
            if let Some(crtc_info) = setup.get_crtc_info(*output) {
                regions.push(xcb::x::Rectangle::from_crtc(&crtc_info));
            }
        }

        // Filter and sort crtc regions.
        let mut valid_regions = regions
            .iter()
            .enumerate()
            .filter_map(|(index, rect)| {
                regions
                    .iter()
                    .enumerate()
                    .all(|(index_other, other)| index == index_other || !rect.is_inside(other))
                    .then_some(*rect)
            })
            .collect::<Vec<_>>();
        valid_regions.sort_by(compare_rectangles);

        let monitors = valid_regions
            .into_iter()
            .map(|rect| {
                let (window, pixmap) = setup.create_window_and_pixmap(
                    rect.x,
                    rect.y,
                    rect.width,
                    bar_height,
                    setup.colormap,
                );

                Monitor {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: bar_height,
                    window,
                    pixmap,
                }
            })
            .collect::<Vec<_>>();

        // Create atoms.
        let atom_names = [
            "_NET_WM_DESKTOP",
            "_NET_WM_WINDOW_TYPE",
            "_NET_WM_WINDOW_TYPE_DOCK",
            "_NET_WM_STATE",
            "_NET_WM_STATE_STICKY",
            "_NET_WM_STRUT_PARTIAL",
            "_NET_WM_STRUT",
        ];
        let atom_cookies = atom_names.map(|name| {
            let request = xcb::x::InternAtom {
                only_if_exists: true,
                name: name.as_bytes(),
            };
            setup.connection.send_request(&request)
        });
        let [atom_desktop, atom_window_type, atom_window_type_dock, atom_state, atom_state_sticky, atom_strut, atom_strut_partial] =
            atom_cookies.map(|cookie| setup.connection.wait_for_reply(cookie).unwrap().atom());

        // Set window properties.
        for monitor in &monitors {
            setup.connection.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: atom_desktop,
                r#type: xcb::x::ATOM_CARDINAL,
                data: &[u32::MAX],
            });

            setup.connection.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: atom_window_type,
                r#type: xcb::x::ATOM_ATOM,
                data: &[atom_window_type_dock],
            });

            setup.connection.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: atom_state,
                r#type: xcb::x::ATOM_ATOM,
                data: &[atom_state_sticky],
            });

            let strut = [
                0,
                0,
                monitor.height,
                0,
                0,
                0,
                0,
                0,
                monitor.x as u16,
                monitor.x as u16 + monitor.width,
                0,
                0,
            ];

            setup.connection.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: atom_strut,
                r#type: xcb::x::ATOM_CARDINAL,
                data: &strut,
            });

            setup.connection.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: atom_strut_partial,
                r#type: xcb::x::ATOM_CARDINAL,
                data: &strut[..4],
            });

            setup.connection.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: xcb::x::ATOM_WM_NAME,
                r#type: xcb::x::ATOM_STRING,
                data: "bananabar".as_bytes(),
            });

            setup.connection.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: xcb::x::ATOM_WM_CLASS,
                r#type: xcb::x::ATOM_STRING,
                data: "bananabar".as_bytes(),
            });
        }

        // This is needed to get the root/depth. The root window has 24bpp, we want 32. Why?
        let reference_drawable = xcb::x::Drawable::Window(monitors[0].window);
        let bg_gc = setup.create_gc(reference_drawable, &[xcb::x::Gc::Foreground(0x00000000)]);
        let fg_gc = setup.create_gc(reference_drawable, &[xcb::x::Gc::Foreground(u32::MAX)]);

        // Make windows visible.
        monitors
            .iter()
            .map(|monitor| {
                setup.connection.send_request_checked(&xcb::x::MapWindow {
                    window: monitor.window,
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|cookie| setup.connection.check_request(cookie).unwrap());

        setup.connection.flush().unwrap();

        Self {
            setup,
            monitors,
            bg_gc,
            fg_gc,
        }
    }

    fn clear_monitors(&self) {
        // Make windows visible.
        self.monitors
            .iter()
            .map(|monitor| {
                self.setup
                    .connection
                    .send_request_checked(&xcb::x::PolyFillRectangle {
                        drawable: xcb::x::Drawable::Pixmap(monitor.pixmap),
                        gc: self.bg_gc,
                        rectangles: &[xcb::x::Rectangle {
                            x: 0,
                            y: 0,
                            width: monitor.width,
                            height: monitor.height,
                        }],
                    })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|cookie| self.setup.connection.check_request(cookie).unwrap());
    }

    fn render_string(&self, text: &str) {
        self.clear_monitors();

        let mut draw = self.setup.new_xft_draw(&self.monitors[0].pixmap);
        draw.draw_string(text);
    }
}

fn main() {
    // TODO handle signals.

    // TODO Use execution path: arg0.
    let _instance_name = "bananabar";

    // Connect to the Xserver and initialize scr
    let bar = Bar::new();

    // TODO Handle ARGS
    // TODO clickable areas.

    loop {
        let mut redraw = false;
        match bar.setup.connection.wait_for_event() {
            Ok(event) => match event {
                xcb::Event::X(event) => {
                    match event {
                        xcb::x::Event::ButtonPress(_) => {
                            for monitor in &bar.monitors {
                                bar.setup.connection.exec_(&xcb::x::PolyFillRectangle {
                                    drawable: xcb::x::Drawable::Pixmap(monitor.pixmap),
                                    gc: bar.bg_gc,
                                    rectangles: &[xcb::x::Rectangle {
                                        x: 0,
                                        y: 0,
                                        width: monitor.width,
                                        height: monitor.height,
                                    }],
                                });
                            }

                            bar.render_string("lolralskdjlaskjdlaskjdlaskdjalskdjlaskdjlaskjdlaskjdlaskjdlaskjdlaskdjlaskjdlaksjdlaksjdlaksjdlaksjdlaskjdlaskjdlaskjdlaskjdofl");
                            redraw = true;
                        }
                        _ => { dbg!(&event); }
                        // xcb::x::Event::KeyRelease(_) => todo!(),
                        // xcb::x::Event::ButtonPress(_) => todo!(),
                        // xcb::x::Event::ButtonRelease(_) => todo!(),
                        // xcb::x::Event::MotionNotify(_) => todo!(),
                        // xcb::x::Event::EnterNotify(_) => todo!(),
                        // xcb::x::Event::LeaveNotify(_) => todo!(),
                        // xcb::x::Event::FocusIn(_) => todo!(),
                        // xcb::x::Event::FocusOut(_) => todo!(),
                        // xcb::x::Event::KeymapNotify(_) => todo!(),
                        // xcb::x::Event::Expose(_) => todo!(),
                        // xcb::x::Event::GraphicsExposure(_) => todo!(),
                        // xcb::x::Event::NoExposure(_) => todo!(),
                        // xcb::x::Event::VisibilityNotify(_) => todo!(),
                        // xcb::x::Event::CreateNotify(_) => todo!(),
                        // xcb::x::Event::DestroyNotify(_) => todo!(),
                        // xcb::x::Event::UnmapNotify(_) => todo!(),
                        // xcb::x::Event::MapNotify(_) => todo!(),
                        // xcb::x::Event::MapRequest(_) => todo!(),
                        // xcb::x::Event::ReparentNotify(_) => todo!(),
                        // xcb::x::Event::ConfigureNotify(_) => todo!(),
                        // xcb::x::Event::ConfigureRequest(_) => todo!(),
                        // xcb::x::Event::GravityNotify(_) => todo!(),
                        // xcb::x::Event::ResizeRequest(_) => todo!(),
                        // xcb::x::Event::CirculateNotify(_) => todo!(),
                        // xcb::x::Event::CirculateRequest(_) => todo!(),
                        // xcb::x::Event::PropertyNotify(_) => todo!(),
                        // xcb::x::Event::SelectionClear(_) => todo!(),
                        // xcb::x::Event::SelectionRequest(_) => todo!(),
                        // xcb::x::Event::SelectionNotify(_) => todo!(),
                        // xcb::x::Event::ColormapNotify(_) => todo!(),
                        // xcb::x::Event::ClientMessage(_) => todo!(),
                        // xcb::x::Event::MappingNotify(_) => todo!(),
                    }
                }
                xcb::Event::RandR(event) => {
                    dbg!(&event);
                }
                xcb::Event::Unknown(event) => {
                    dbg!(&event);
                }
            },
            Err(err) => {
                panic!("{:?}", err);
            }
        }

        if redraw {
            for monitor in &bar.monitors {
                bar.setup.connection.exec_(&xcb::x::CopyArea {
                    src_drawable: xcb::x::Drawable::Pixmap(monitor.pixmap),
                    dst_drawable: xcb::x::Drawable::Window(monitor.window),
                    gc: bar.bg_gc,
                    src_x: 0,
                    src_y: 0,
                    dst_x: 0,
                    dst_y: 0,
                    width: monitor.width,
                    height: monitor.height,
                });
            }
        }

        bar.setup.connection.flush().unwrap();
    }
}
