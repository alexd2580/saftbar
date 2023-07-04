mod analyse;
mod connection;
mod setup;
mod xft;

use analyse::{analyse_string, ColoredText, InputAnalysis, SingleDisplay};
use setup::{compare_rectangles, Rectangle, Setup};
use xcb::Xid;
use xft::Xft;

struct Monitor {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    window: xcb::x::Window,
    pixmap: xcb::x::Pixmap,
}

struct Bar {
    setup: Setup,
    xft: Xft,
    font: *mut x11::xft::XftFont,
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
            setup.send_request(&request)
        });
        let [atom_desktop, atom_window_type, atom_window_type_dock, atom_state, atom_state_sticky, atom_strut, atom_strut_partial] =
            atom_cookies.map(|cookie| setup.wait_for_reply(cookie).unwrap().atom());

        // Set window properties.
        for monitor in &monitors {
            setup.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: atom_desktop,
                r#type: xcb::x::ATOM_CARDINAL,
                data: &[u32::MAX],
            });

            setup.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: atom_window_type,
                r#type: xcb::x::ATOM_ATOM,
                data: &[atom_window_type_dock],
            });

            setup.exec_(&xcb::x::ChangeProperty {
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

            setup.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: atom_strut,
                r#type: xcb::x::ATOM_CARDINAL,
                data: &strut,
            });

            setup.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: atom_strut_partial,
                r#type: xcb::x::ATOM_CARDINAL,
                data: &strut[..4],
            });

            setup.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: xcb::x::ATOM_WM_NAME,
                r#type: xcb::x::ATOM_STRING,
                data: "saftladen".as_bytes(),
            });

            setup.exec_(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: monitor.window,
                property: xcb::x::ATOM_WM_CLASS,
                r#type: xcb::x::ATOM_STRING,
                data: "saftladen".as_bytes(),
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
                setup.send_request_checked(&xcb::x::MapWindow {
                    window: monitor.window,
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|cookie| setup.check_request(cookie).unwrap());

        setup.flush().unwrap();

        let mut xft = setup.create_xft();
        let font = b"UbuntuMono Nerd Font:size=12\0";
        let font = xft.font(font);

        Self {
            setup,
            xft,
            font,
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
                self.setup.send_request_checked(&xcb::x::PolyFillRectangle {
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
            .for_each(|cookie| self.setup.check_request(cookie).unwrap());
    }

    fn render_string(&mut self, _text: &str) {
        self.clear_monitors();
        let InputAnalysis(per_monitor) = analyse_string();
        for (index, monitor) in self.monitors.iter().enumerate() {
            let draw = self.xft.new_draw(monitor.pixmap.resource_id() as u64);
            let Some(Some(SingleDisplay { left, right })) = per_monitor.get(index) else {
                continue;
            };

            for ColoredText { text, fg, bg } in left {
                let fg = self.xft.color(*fg);
                self.xft.draw_string(&text, draw.draw, fg, self.font);
            }
            for ColoredText { text, fg, bg } in right {
                let fg = self.xft.color(*fg);
                self.xft.draw_string(&text, draw.draw, fg, self.font);
            }
        }

        // let draw = self.xft.new_draw(self.monitors[0].pixmap.resource_id() as u64);
        // draw.draw_string(text);
    }
}

fn main() {
    // TODO handle signals.

    // TODO Use execution path: arg0.
    let _instance_name = "saftladen";

    // Connect to the Xserver and initialize scr
    let mut bar = Bar::new();

    // TODO Handle ARGS
    // TODO clickable areas.

    loop {
        let mut redraw = false;
        match bar.setup.wait_for_event() {
            Ok(event) => match event {
                xcb::Event::X(event) => {
                    match event {
                        xcb::x::Event::ButtonPress(_) => {
                            for monitor in &bar.monitors {
                                bar.setup.exec_(&xcb::x::PolyFillRectangle {
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

                            bar.render_string("lol");
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
                bar.setup.exec_(&xcb::x::CopyArea {
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

        bar.setup.flush().unwrap();
    }
}
