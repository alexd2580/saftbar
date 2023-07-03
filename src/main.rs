use std::{cmp::Ordering, ops::Deref, time::Duration};

use xcb::Xid;

struct Monitor {
    rect: xcb::x::Rectangle,
    window: xcb::x::Window,
    pixmap: xcb::x::Pixmap,
}

trait Rectangle {
    fn from_crtc(crtc: &xcb::randr::GetCrtcInfoReply) -> Self;
    fn is_inside(&self, rect: &Self) -> bool;
}

impl Rectangle for xcb::x::Rectangle {
    fn from_crtc(crtc: &xcb::randr::GetCrtcInfoReply) -> Self {
        xcb::x::Rectangle {
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

trait Connection {
    fn new() -> Self;

    fn request_without_response<Request>(&self, request: &Request)
    where
        Request: xcb::RequestWithoutReply;

    fn request_with_response<Request>(
        &self,
        request: &Request,
    ) -> <<Request as xcb::Request>::Cookie as xcb::CookieWithReplyChecked>::Reply
    where
        Request: xcb::Request,
        <Request as xcb::Request>::Cookie: xcb::CookieWithReplyChecked;
}

impl Connection for xcb::Connection {
    fn new() -> Self {
        let extensions = [xcb::Extension::RandR];
        let (connection, preferred_screen_index) =
            xcb::Connection::connect_with_extensions(None, &extensions, &[]).unwrap();

        assert_eq!(preferred_screen_index, 0);

        connection
    }

    fn request_without_response<Request>(&self, request: &Request)
    where
        Request: xcb::RequestWithoutReply,
    {
        self.send_and_check_request(request).unwrap();
    }

    fn request_with_response<Request>(
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
}

struct X {
    connection: xcb::Connection,
    colormap: xcb::x::Colormap,
}

fn connect_to_x() -> X {
    let connection = xcb::Connection::new();

    // How the layout looks like.
    let setup_info = connection.get_setup();
    assert_eq!(setup_info.roots().count(), 1);

    // The root screen - rendering canvas.
    let screen = setup_info.roots().nth(0).unwrap();

    // The root window, which is essentially a rect.
    let root_window = screen.root();
    let root_visual = screen
        .allowed_depths()
        .find_map(|depth| (depth.depth() == 32).then(|| depth.visuals()[0].visual_id()))
        .unwrap();

    let colormap: xcb::x::Colormap = connection.generate_id();
    let request = xcb::x::CreateColormap {
        alloc: xcb::x::ColormapAlloc::None,
        mid: colormap,
        window: root_window,
        visual: root_visual,
    };
    connection.send_and_check_request(&request).unwrap();

    let width = screen.width_in_pixels();
    let height = 20;

    let request = xcb::randr::GetScreenResourcesCurrent {
        window: root_window,
    };
    let screen_resources = connection.request_with_response(&request);
    let outputs = screen_resources.outputs();

    let config_timestamp = xcb::x::CURRENT_TIME;

    // Get output regions.
    let mut regions = Vec::new();
    for output in outputs {
        let output = *output;

        let request = xcb::randr::GetOutputInfo {
            output,
            config_timestamp,
        };
        let output_info = connection.request_with_response(&request);

        let crtc = output_info.crtc();
        if crtc.is_none() || output_info.connection() != xcb::randr::Connection::Connected {
            // Something fishy, skup this output.
            continue;
        }

        let request = xcb::randr::GetCrtcInfo {
            crtc,
            config_timestamp,
        };
        let crtc_info = connection.request_with_response(&request);

        regions.push(xcb::x::Rectangle::from_crtc(&crtc_info));
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
            let window = connection.generate_id();
            let depth = 32; // TODO (visual == scr->root_visual) ? XCB_COPY_FROM_PARENT : 32;
            let request = xcb::x::CreateWindow {
                depth,
                wid: window,
                parent: root_window,
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height,
                border_width: 0,
                class: xcb::x::WindowClass::InputOutput,
                visual: root_visual,
                value_list: &[
                    xcb::x::Cw::BackPixel(screen.black_pixel()),
                    xcb::x::Cw::BorderPixel(screen.black_pixel()),
                    xcb::x::Cw::OverrideRedirect(false), // EMWH noncompliant
                    xcb::x::Cw::EventMask(
                        xcb::x::EventMask::EXPOSURE | xcb::x::EventMask::BUTTON_PRESS,
                    ),
                    xcb::x::Cw::Colormap(colormap),
                ],
            };
            connection.request_without_response(&request);

            let pixmap = connection.generate_id();
            let request = xcb::x::CreatePixmap {
                depth,
                pid: pixmap,
                drawable: xcb::x::Drawable::Window(window),
                width: rect.width,
                height,
            };
            connection.request_without_response(&request);

            let mut mon_rect = rect.clone();
            dbg!(&mon_rect);
            mon_rect.height = height;
            Monitor {
                rect: mon_rect,
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
        connection.send_request(&request)
    });
    let [atom_desktop, atom_window_type, atom_window_type_dock, atom_state, atom_state_sticky, atom_strut, atom_strut_partial] =
        atom_cookies.map(|cookie| connection.wait_for_reply(cookie).unwrap().atom());

    // Set window properties.
    for monitor in &monitors {
        let request = xcb::x::ChangeProperty {
            mode: xcb::x::PropMode::Replace,
            window: monitor.window,
            property: atom_desktop,
            r#type: xcb::x::ATOM_CARDINAL,
            data: &[u32::MAX],
        };
        connection.request_without_response(&request);

        let request = xcb::x::ChangeProperty {
            mode: xcb::x::PropMode::Replace,
            window: monitor.window,
            property: atom_window_type,
            r#type: xcb::x::ATOM_ATOM,
            data: &[atom_window_type_dock],
        };
        connection.request_without_response(&request);

        let request = xcb::x::ChangeProperty {
            mode: xcb::x::PropMode::Replace,
            window: monitor.window,
            property: atom_state,
            r#type: xcb::x::ATOM_ATOM,
            data: &[atom_state_sticky],
        };
        connection.request_without_response(&request);

        let strut = [
            0,
            0,
            monitor.rect.height,
            0,
            0,
            0,
            0,
            0,
            monitor.rect.x as u16,
            monitor.rect.x as u16 + monitor.rect.width,
            10,
            11,
        ];

        let request = xcb::x::ChangeProperty {
            mode: xcb::x::PropMode::Replace,
            window: monitor.window,
            property: atom_strut,
            r#type: xcb::x::ATOM_CARDINAL,
            data: &strut,
        };
        connection.request_without_response(&request);

        let request = xcb::x::ChangeProperty {
            mode: xcb::x::PropMode::Replace,
            window: monitor.window,
            property: atom_strut_partial,
            r#type: xcb::x::ATOM_CARDINAL,
            data: &strut[..4],
        };
        connection.request_without_response(&request);

        let request = xcb::x::ChangeProperty {
            mode: xcb::x::PropMode::Replace,
            window: monitor.window,
            property: xcb::x::ATOM_WM_NAME,
            r#type: xcb::x::ATOM_STRING,
            data: "bananabar".as_bytes(),
        };
        connection.request_without_response(&request);

        let request = xcb::x::ChangeProperty {
            mode: xcb::x::PropMode::Replace,
            window: monitor.window,
            property: xcb::x::ATOM_WM_CLASS,
            r#type: xcb::x::ATOM_STRING,
            data: "bananabar".as_bytes(),
        };
        connection.request_without_response(&request);
    }

    let draw_gc = connection.generate_id();
    let request = xcb::x::CreateGc {
        cid: draw_gc,
        // Why 0th pixmap?
        drawable: xcb::x::Drawable::Pixmap(monitors[0].pixmap),
        value_list: &[xcb::x::Gc::Foreground(u32::MAX)],
    };
    connection.request_without_response(&request);

    let clear_gc = connection.generate_id();
    let request = xcb::x::CreateGc {
        cid: clear_gc,
        // Why 0th pixmap?
        drawable: xcb::x::Drawable::Pixmap(monitors[0].pixmap),
        value_list: &[xcb::x::Gc::Foreground(u32::MAX)],
    };
    connection.request_without_response(&request);

    let attr_gc = connection.generate_id();
    let request = xcb::x::CreateGc {
        cid: attr_gc,
        // Why 0th pixmap?
        drawable: xcb::x::Drawable::Pixmap(monitors[0].pixmap),
        value_list: &[xcb::x::Gc::Foreground(u32::MAX)],
    };
    connection.request_without_response(&request);

    // Make windows visible.
    for monitor in &monitors {
        let request = xcb::x::PolyFillRectangle {
            drawable: xcb::x::Drawable::Pixmap(monitor.pixmap),
            gc: clear_gc,
            rectangles: &[xcb::x::Rectangle {
                x: 0,
                y: 0,
                ..monitor.rect
            }],
        };
        dbg!(&request);
        connection.request_without_response(&request);

        let request = xcb::x::MapWindow {
            window: monitor.window,
        };
        connection.request_without_response(&request);
    }

    connection.flush().unwrap();

    X {
        connection,
        colormap,
    }
}

fn main() {
    // TODO handle signals.

    // TODO Use execution path: arg0.
    let instance_name = "bananabar";

    // Connect to the Xserver and initialize scr
    let x = connect_to_x();

    // TODO Handle ARGS
    // TODO clickable areas.

    // // Do the heavy lifting
    // init(wm_name, instance_name);
    // // The string is strdup'd when the command line arguments are parsed
    // free(wm_name);
    // // The string is strdup'd when stripping argv[0]
    // free(instance_name);
    // // Get the fd to Xserver
    // pollin[1].fd = xcb_get_file_descriptor(c);
    //
    // // Prevent fgets to block
    // fcntl(STDIN_FILENO, F_SETFL, O_NONBLOCK);
    //
    // loop {
    //     // If connection is in error state, then it has been shut down.
    //     if xcb_connection_has_error(c) {
    //         break;
    //     }
    //
    //     let redraw = false;
    //
    //     // If new input:
    //     // parse the input and prepare redraw.
    //
    //     // Check X for events.
    //     // if(pollin[1].revents & POLLIN) { // The event comes from the Xorg server
    //     //     while((ev = xcb_poll_for_event(c))) {
    //     //         expose_ev = (xcb_expose_event_t*)ev;
    //     //
    //     //         switch(ev->response_type & 0x7F) {
    //     //         case XCB_EXPOSE:
    //     //             if(expose_ev->count == 0)
    //     //                 redraw = true;
    //     //             break;
    //     //         case XCB_BUTTON_PRESS:
    //     //             press_ev = (xcb_button_press_event_t*)ev;
    //     //             {
    //     //                 area_t* area = area_get(press_ev->event, press_ev->detail, press_ev->event_x);
    //     //                 // Respond to the click
    //     //                 if(area) {
    //     //                     (void)write(STDOUT_FILENO, area->cmd, strlen(area->cmd));
    //     //                     (void)write(STDOUT_FILENO, "\n", 1);
    //     //                 }
    //     //             }
    //     //             break;
    //     //         }
    //     //
    //     //         free(ev);
    //     //     }
    //     // }
    //
    //     // Copy our temporary pixmap onto the window
    //     if redraw {
    //         // for(monitor_t* mon = monhead; mon; mon = mon->next) {
    //         //     xcb_copy_area(c, mon->pixmap, mon->window, gc[GC_DRAW], 0, 0, 0, 0, mon->width, bh);
    //         // }
    //     }
    //
    //     xcb_flush(c);
    // }

    std::thread::sleep(Duration::from_secs(5));
}
