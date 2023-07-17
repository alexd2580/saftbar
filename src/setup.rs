use std::{cmp::Ordering, ptr::null_mut};

use crate::connection::Connection;
use crate::xft::Xft;

use xcb::Xid;
use xcb::{randr, x};

#[derive(Clone)]
pub struct Rectangle {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl From<&randr::GetCrtcInfoReply> for Rectangle {
    fn from(value: &randr::GetCrtcInfoReply) -> Self {
        Self {
            x: value.x().try_into().unwrap(),
            y: value.y().try_into().unwrap(),
            w: value.width().into(),
            h: value.height().into(),
        }
    }
}

impl Rectangle {
    pub fn is_inside(&self, rect: &Rectangle) -> bool {
        self.x >= rect.x
            && self.x + self.w <= rect.x + rect.w
            && self.y >= rect.y
            && self.y + self.h <= rect.y + rect.h
    }
}

// Order rects from left to right, then from top to bottom.
// Edge cases for overlapping screens.
pub fn compare_rectangles(a: &Rectangle, b: &Rectangle) -> Ordering {
    if a.x == b.x {
        (a.y + a.h).cmp(&b.y)
    } else {
        a.x.cmp(&b.x)
    }
}

#[derive(Debug)]
pub enum PropertyData<'a> {
    Cardinal(&'a [u32]),
    Atom(&'a [x::Atom]),
    String(&'a [u8]),
}

// The following are structs holding the data for a pipelined version of the respective request.

#[derive(Debug)]
pub struct ChangeProperty<'a>(pub x::Atom, pub PropertyData<'a>);

#[derive(Debug)]
pub struct MapWindow(pub x::Window);

#[derive(Debug)]
pub struct FillRect(
    pub x::Drawable,
    pub x::Gcontext,
    pub u32,
    pub u32,
    pub u32,
    pub u32,
);

#[derive(Debug)]
pub struct CopyArea(
    pub x::Pixmap,
    pub x::Window,
    pub x::Gcontext,
    pub u32,
    pub u32,
);

pub struct Setup {
    // width: u32,
    // height: u32,

    // Note the reverse drop order! Children first.
    pub colormap: x::Colormap,
    visual: *mut x11::xlib::Visual,
    visual_id: u32,
    root_window: x::Window,
    connection: Connection,
}

impl Setup {
    /// Create the basic setup for dealing with windows.
    pub fn new() -> Self {
        let connection = Connection::new();

        // How the layout looks like.
        let setup_info = connection.get_setup();
        assert_eq!(setup_info.roots().count(), 1);

        // The root screen - rendering canvas.
        let screen = setup_info.roots().next().expect("Failed to get 0th screen");

        // The root window, which is essentially a rect.
        let root_window = screen.root();
        let visual_id = screen
            .allowed_depths()
            .find_map(|depth| (depth.depth() == 32).then(|| depth.visuals()[0].visual_id()))
            .expect("Failed to find 32bit depth visual");

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
                std::ptr::addr_of_mut!(visual_info_mask),
                std::ptr::addr_of_mut!(result),
            )
        };
        let visual_info = unsafe { *visual_info };
        assert_eq!(visual_info.visualid, u64::from(visual_id));
        let visual = visual_info.visual;

        // let width = u32::from(screen.width_in_pixels());
        // let height = u32::from(screen.height_in_pixels());

        let colormap: x::Colormap = connection.generate_id();
        connection
            .exec_(&x::CreateColormap {
                alloc: x::ColormapAlloc::None,
                mid: colormap,
                window: root_window,
                visual: visual_id,
            })
            .expect("Failed to create colormap");

        Self {
            // width,
            // height,
            colormap,
            visual,
            visual_id,
            root_window,
            connection,
        }
    }

    pub fn get_screen_resources(&self) -> randr::GetScreenResourcesCurrentReply {
        self.connection
            .exec(&randr::GetScreenResourcesCurrent {
                window: self.root_window,
            })
            .expect("Failed to get screen resources")
    }

    /// Retrieve the crtc info for a given output.
    pub fn get_crtc_info(&self, output: randr::Output) -> Option<randr::GetCrtcInfoReply> {
        let config_timestamp = x::CURRENT_TIME;
        let output_info = self
            .connection
            .exec(&randr::GetOutputInfo {
                output,
                config_timestamp,
            })
            .expect("Failed to get output info");

        let crtc = output_info.crtc();
        // Require that crtcs are connected and not none.
        let valid_crtc =
            !crtc.is_none() && output_info.connection() == randr::Connection::Connected;
        valid_crtc.then(|| {
            self.connection
                .exec(&randr::GetCrtcInfo {
                    crtc,
                    config_timestamp,
                })
                .expect("Failed to get crtc info")
        })
    }

    /// Send and await multiple void requests in parallel.
    ///
    /// Often you want to perform multiple actions one after another and retrieve their results (or non
    /// if void). If these actions don't depend on each other then you can send all requests first,
    /// let X process these and then retrieve the results. This way you reduce the amount of round
    /// trips at best by N-1 times.
    ///
    /// I have not benchmarked this function to determine whether this makes any sense at all
    /// nowadays with modern hardware, but all the "best practice" examples out there do this, so
    /// it can't be totally wrong, can it?
    ///
    /// TODO:
    /// Maybe redefine this function to work with anything iterator-able?
    fn pipeline_requests<T: std::fmt::Debug>(
        &self,
        data: &[T],
        send_request: impl Fn(&T) -> xcb::VoidCookieChecked,
    ) {
        data.iter()
            .map(send_request)
            .collect::<Vec<_>>()
            .into_iter()
            .zip(data.iter())
            .for_each(|(cookie, data)| {
                if let Err(err) = self.connection.check_request(cookie) {
                    panic!("Request failed: {data:?}; {err}");
                }
            });
    }

    pub fn create_window_and_pixmap(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        colormap: x::Colormap,
    ) -> (x::Window, x::Pixmap) {
        let window = self.connection.generate_id();
        let depth = 32; // TODO (visual == scr->root_visual) ? XCB_COPY_FROM_PARENT : 32;

        let width = width.try_into().unwrap();
        let height = height.try_into().unwrap();

        self.connection
            .exec_(&x::CreateWindow {
                depth,
                wid: window,
                parent: self.root_window,
                x: x.try_into().unwrap(),
                y: y.try_into().unwrap(),
                width,
                height,
                border_width: 0,
                class: x::WindowClass::InputOutput,
                visual: self.visual_id,
                value_list: &[
                    x::Cw::BackPixel(0x0000_0000),
                    x::Cw::BorderPixel(0x0000_0000),
                    x::Cw::OverrideRedirect(false), // EMWH noncompliant (TODO what do i mean?)
                    x::Cw::EventMask(x::EventMask::EXPOSURE | x::EventMask::BUTTON_PRESS),
                    x::Cw::Colormap(colormap),
                ],
            })
            .expect("Failed to create window");

        let pixmap = self.connection.generate_id();
        self.connection
            .exec_(&x::CreatePixmap {
                depth,
                pid: pixmap,
                drawable: x::Drawable::Window(window),
                width,
                height,
            })
            .expect("Failed to create pixmap");

        (window, pixmap)
    }

    pub fn get_atoms<const N: usize>(&self, atom_names: &[&str; N]) -> [x::Atom; N] {
        let conn = &self.connection;
        atom_names
            .map(|name| {
                let request = x::InternAtom {
                    only_if_exists: false,
                    name: name.as_bytes(),
                };
                (name, conn.send_request(&request))
            })
            .map(|(name, cookie)| {
                conn.wait_for_reply(cookie)
                    .unwrap_or_else(|err| panic!("Failed to get atom '{name}'; {err}"))
                    .atom()
            })
    }

    pub fn replace_properties(&self, window: x::Window, properties: &[ChangeProperty]) {
        use PropertyData::{Atom, Cardinal, String};

        let conn = &self.connection;
        let mode = x::PropMode::Replace;
        self.pipeline_requests(
            properties,
            |&ChangeProperty(property, ref data)| match data {
                Cardinal(data) => conn.send_request_checked(&x::ChangeProperty {
                    mode,
                    window,
                    property,
                    r#type: x::ATOM_CARDINAL,
                    data,
                }),
                Atom(data) => conn.send_request_checked(&x::ChangeProperty {
                    mode,
                    window,
                    property,
                    r#type: x::ATOM_ATOM,
                    data,
                }),
                String(data) => conn.send_request_checked(&x::ChangeProperty {
                    mode,
                    window,
                    property,
                    r#type: x::ATOM_STRING,
                    data,
                }),
            },
        );
    }

    /// Display windows.
    pub fn map_windows(&self, windows: &[MapWindow]) {
        self.pipeline_requests(windows, |&MapWindow(window)| {
            self.connection
                .send_request_checked(&x::MapWindow { window })
        });
    }

    pub fn create_gc(&self, drawable: x::Drawable, value_list: &[x::Gc]) -> x::Gcontext {
        let cid = self.connection.generate_id();
        self.connection
            .exec_(&x::CreateGc {
                cid,
                drawable,
                value_list,
            })
            .expect("Failed to create graphics context");
        cid
    }

    pub fn create_xft(&self) -> Xft {
        Xft::new(
            self.connection.get_raw_dpy(),
            self.visual,
            u64::from(self.colormap.resource_id()),
        )
    }

    pub fn fill_rects(&self, rects: &[FillRect]) {
        self.pipeline_requests(rects, |&FillRect(drawable, gc, x, y, w, h)| {
            self.connection.send_request_checked(&x::PolyFillRectangle {
                drawable,
                gc,
                rectangles: &[x::Rectangle {
                    x: x.try_into().unwrap(),
                    y: y.try_into().unwrap(),
                    width: w.try_into().unwrap(),
                    height: h.try_into().unwrap(),
                }],
            })
        });
    }

    pub fn copy_areas(&self, areas: &[CopyArea]) {
        self.pipeline_requests(areas, |&CopyArea(pixmap, window, gc, w, h)| {
            self.connection.send_request_checked(&x::CopyArea {
                src_drawable: x::Drawable::Pixmap(pixmap),
                dst_drawable: x::Drawable::Window(window),
                gc,
                src_x: 0,
                src_y: 0,
                dst_x: 0,
                dst_y: 0,
                width: w.try_into().unwrap(),
                height: h.try_into().unwrap(),
            })
        });
    }

    pub fn flush(&self) {
        self.connection
            .flush()
            .expect("Failed to flush xcb connection");
    }
}
