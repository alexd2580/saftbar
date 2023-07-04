mod analyse;
mod connection;
mod setup;
mod xft;

use analyse::{analyse_string, ColoredText, InputAnalysis, SingleDisplay};
use setup::{compare_rectangles, Rectangle, Setup};
use xcb::Xid;
use xft::{Font, Xft};

struct Monitor {
    x: u32,
    y: u32,
    w: u32,
    window: xcb::x::Window,
    pixmap: xcb::x::Pixmap,
}

struct Bar {
    height: u32,
    setup: Setup,
    xft: Xft,
    font: Font,
    monitors: Vec<Monitor>,
    clear_gc: xcb::x::Gcontext,
    color_gc: xcb::x::Gcontext,
}

impl Bar {
    fn new() -> Self {
        let setup = Setup::new();

        let screen_resources = setup.get_screen_resources();
        let outputs = screen_resources.outputs();

        // Get output regions.
        let mut regions = Vec::new();
        for output in outputs {
            if let Some(crtc_info) = setup.get_crtc_info(*output) {
                regions.push(Rectangle::from(&crtc_info));
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
                    .then_some(rect.clone())
            })
            .collect::<Vec<_>>();
        valid_regions.sort_by(compare_rectangles);

        let height = 16;
        let monitors = valid_regions
            .into_iter()
            .map(|Rectangle { x, y, w, .. }| {
                let (window, pixmap) =
                    setup.create_window_and_pixmap(x, y, w, height, setup.colormap);

                Monitor {
                    x,
                    y,
                    w,
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
                height,
                0,
                0,
                0,
                0,
                0,
                monitor.x,
                monitor.x + monitor.w,
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
        let clear_gc = setup.create_gc(reference_drawable, &[xcb::x::Gc::Foreground(0x00000000)]);
        let color_gc = setup.create_gc(reference_drawable, &[xcb::x::Gc::Foreground(u32::MAX)]);

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
        let font = b"UbuntuMono Nerd Font:size=12:antialias=true:hinting=false\0";

        let font = xft.create_font(font);

        Self {
            height,
            setup,
            xft,
            font,
            monitors,
            clear_gc,
            color_gc,
        }
    }

    fn clear_monitors(&self) {
        // Make windows visible.
        self.monitors
            .iter()
            .map(|monitor| {
                self.setup.send_request_checked(&xcb::x::PolyFillRectangle {
                    drawable: xcb::x::Drawable::Pixmap(monitor.pixmap),
                    gc: self.clear_gc,
                    rectangles: &[xcb::x::Rectangle {
                        x: 0,
                        y: 0,
                        width: monitor.w.try_into().unwrap(),
                        height: self.height.try_into().unwrap(),
                    }],
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|cookie| self.setup.check_request(cookie).unwrap());
    }

    fn render_string(&mut self, _text: &str) {
        let InputAnalysis(per_monitor) = analyse_string();
        for (index, monitor) in self.monitors.iter().enumerate() {
            let draw = self.xft.new_draw(monitor.pixmap.resource_id() as u64);
            let Some(Some(SingleDisplay { left, right })) = per_monitor.get(index) else {
                continue;
            };

            // Left aligned.
            let mut cursor_offset = 0;
            for ColoredText { text, fg, bg } in left {
                let width = self.xft.string_cursor_offset(&text, &self.font);

                let r = u32::from(bg.0) >> 8;
                let g = u32::from(bg.1) >> 8;
                let b = u32::from(bg.2) >> 8;
                let a = u32::from(bg.3) >> 8;
                let color = r << 24 | g << 16 | b << 8 | a;
                self.setup.exec_(&xcb::x::ChangeGc {
                    gc: self.color_gc,
                    value_list: &[xcb::x::Gc::Foreground(color)],
                });

                self.setup.exec_(&xcb::x::PolyFillRectangle {
                    drawable: xcb::x::Drawable::Pixmap(monitor.pixmap),
                    gc: self.color_gc,
                    rectangles: &[xcb::x::Rectangle {
                        x: cursor_offset.try_into().unwrap(),
                        y: 0,
                        width: width.try_into().unwrap(),
                        height: self.height.try_into().unwrap(),
                    }],
                });

                let fg = self.xft.create_color(*fg);
                self.xft.draw_string(
                    &text,
                    &draw,
                    &fg,
                    &self.font,
                    self.height as u32,
                    cursor_offset,
                );
                cursor_offset += width;
            }

            // Right aligned.
            let mut text_width = 0;
            let cursor_offsets = right
                .iter()
                .map(|text| {
                    let cursor_offset = self.xft.string_cursor_offset(&text.text, &self.font);
                    text_width += cursor_offset;
                    cursor_offset
                })
                .collect::<Vec<_>>();

            let mut cursor_offset = monitor.w - text_width;
            for (ColoredText { text, fg, .. }, offset) in
                right.iter().zip(cursor_offsets.into_iter())
            {
                let fg = self.xft.create_color(*fg);
                self.xft
                    .draw_string(text, &draw, &fg, &self.font, self.height, cursor_offset);
                cursor_offset += offset;
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
                            bar.clear_monitors();
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
                    gc: bar.clear_gc,
                    src_x: 0,
                    src_y: 0,
                    dst_x: 0,
                    dst_y: 0,
                    width: monitor.w.try_into().unwrap(),
                    height: bar.height.try_into().unwrap(),
                });
            }
        }

        bar.setup.flush().unwrap();
    }
}
