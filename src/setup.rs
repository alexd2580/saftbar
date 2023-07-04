use std::{cmp::Ordering, ptr::null_mut, ops::Deref};

use crate::{connection::Connection, xft::Xft};

use xcb::Xid;

pub trait Rectangle {
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
pub fn compare_rectangles(a: &xcb::x::Rectangle, b: &xcb::x::Rectangle) -> Ordering {
    if a.x != b.x {
        a.x.cmp(&b.x)
    } else {
        (a.y + a.height as i16).cmp(&b.y)
    }
}

pub struct Setup {
    connection: Connection,
    root_window: xcb::x::Window,
    width: u16,
    height: u16,
    visual_id: u32,
    visual: *mut x11::xlib::Visual,
    pub colormap: xcb::x::Colormap,
}

impl Deref for Setup {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl Setup {
    pub fn new() -> Self {
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

    pub fn get_screen_resources(&self) -> xcb::randr::GetScreenResourcesCurrentReply {
        self.connection
            .exec(&xcb::randr::GetScreenResourcesCurrent {
                window: self.root_window,
            })
    }

    pub fn get_crtc_info(
        &self,
        output: xcb::randr::Output,
    ) -> Option<xcb::randr::GetCrtcInfoReply> {
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

    pub fn create_window_and_pixmap(
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

    pub fn create_gc(
        &self,
        drawable: xcb::x::Drawable,
        value_list: &[xcb::x::Gc],
    ) -> xcb::x::Gcontext {
        let cid = self.connection.generate_id();
        self.connection.exec_(&xcb::x::CreateGc {
            cid,
            drawable,
            value_list,
        });
        cid
    }

    pub fn create_xft(&self) -> Xft {
        Xft::new(
            self.connection.get_raw_dpy(),
            self.visual,
            self.colormap.resource_id() as u64,
        )
    }
}
