mod connection;
mod setup;
mod xft;

use std::collections::HashMap;

use setup::{compare_rectangles, Rectangle, Setup};
use xcb::{x, Xid};
use xft::{Font, Xft, RGBA};

struct Monitor {
    x: u32,
    y: u32,
    w: u32,
    window: xcb::x::Window,
    pixmap: x::Pixmap,
}

pub enum Alignment {
    Left,
    Right,
}

pub struct ColoredText {
    pub text: String,
    pub fg: RGBA,
    pub bg: RGBA,
}

pub struct Bar {
    height: u32,
    setup: Setup,
    xft: Xft,
    font: Font,
    monitors: Vec<Monitor>,
    clear_gc: x::Gcontext,
    color_gcs: HashMap<RGBA, x::Gcontext>,
}

impl Bar {
    pub fn new() -> Self {
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

        let height = 20;
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

        // Set EWMH or something values.
        {
            // Create atoms.
            let [desktop, window_type, window_type_dock, state, state_sticky, strut, strut_partial] =
                setup.get_atoms(&[
                    "_NET_WM_DESKTOP",
                    "_NET_WM_WINDOW_TYPE",
                    "_NET_WM_WINDOW_TYPE_DOCK",
                    "_NET_WM_STATE",
                    "_NET_WM_STATE_STICKY",
                    "_NET_WM_STRUT_PARTIAL",
                    "_NET_WM_STRUT",
                ]);

            use setup::PropertyData::{Atom, Cardinal, String};

            let window_type_dock = [window_type_dock];
            let state_sticky = [state_sticky];
            let name_bytes = "saftbar".as_bytes();
            let properties = [
                (desktop, Cardinal(&[u32::MAX])),
                (window_type, Atom(&window_type_dock)),
                (state, Atom(&state_sticky)),
                (x::ATOM_WM_NAME, String(name_bytes)),
                (x::ATOM_WM_CLASS, String(name_bytes)),
            ];

            // Set window properties.
            for monitor in &monitors {
                setup.replace_properties(monitor.window, &properties);

                let h = height;
                let sx = monitor.x;
                let ex = sx + monitor.w;
                let strut_data = [0, 0, h, 0, 0, 0, 0, 0, sx, ex, 0, 0];
                let monitor_properties = [
                    (strut, Cardinal(&strut_data[..4])),
                    (strut_partial, Cardinal(&strut_data)),
                ];
                setup.replace_properties(monitor.window, &monitor_properties);
            }
        }

        // This is needed to get the root/depth. The root window has 24bpp, we want 32. Why?
        let reference_drawable = x::Drawable::Window(monitors[0].window);
        let clear_gc = setup.create_gc(reference_drawable, &[x::Gc::Foreground(0x00000000)]);

        // Make windows visible.
        setup.map_windows(
            &monitors
                .iter()
                .map(|monitor| monitor.window)
                .collect::<Vec<_>>(),
        );

        setup.flush();

        // Initialize font.
        let mut xft = setup.create_xft();

        // let font_family = "UbuntuMono Nerd Font";
        let font_family = "FiraCode Nerd Font Propo";
        let font = {
            let font_params = ":pixelsize=16:antialias=true:hinting=true";
            let font_pattern = format!("{font_family}{font_params}\0");
            xft.create_font(&font_pattern)
        };

        Self {
            height,
            setup,
            xft,
            font,
            monitors,
            clear_gc,
            color_gcs: HashMap::new(),
        }
    }

    fn cache_color(&mut self, reference_drawable: x::Drawable, rgba: &RGBA) {
        if self.color_gcs.get(rgba).is_none() {
            let r = u32::from(rgba.0) >> 8;
            let g = u32::from(rgba.1) >> 8;
            let b = u32::from(rgba.2) >> 8;
            let a = u32::from(rgba.3) >> 8;
            let color = b | g << 8 | r << 16 | a << 24;

            let gc = self
                .setup
                .create_gc(reference_drawable, &[x::Gc::Foreground(color)]);

            self.color_gcs.insert(*rgba, gc);
        }
    }

    fn get_color(&self, rgba: &RGBA) -> x::Gcontext {
        *self.color_gcs.get(rgba).unwrap()
    }

    fn clear_monitors(&self) {
        self.setup.fill_rects(
            &self
                .monitors
                .iter()
                .map(|monitor| {
                    (
                        x::Drawable::Pixmap(monitor.pixmap),
                        self.clear_gc,
                        0,
                        0,
                        monitor.w,
                        self.height,
                    )
                })
                .collect::<Vec<_>>(),
        );
    }

    fn cache_colors(&mut self, monitor_index: usize, texts: &[ColoredText]) {
        let pixmap = self.monitors[monitor_index].pixmap;
        let drawable = x::Drawable::Pixmap(pixmap);
        texts
            .iter()
            .for_each(|item| self.cache_color(drawable, &item.bg));
    }

    fn render_handles(&self, monitor_index: usize) -> (x::Drawable, xft::Draw, u32) {
        let monitor = &self.monitors[monitor_index];
        let pixmap = monitor.pixmap;
        (
            x::Drawable::Pixmap(pixmap),
            self.xft.new_draw(pixmap.resource_id() as u64),
            monitor.w,
        )
    }

    fn render_string_left(&self, monitor_index: usize, texts: &[ColoredText]) {
        let (draw, text_draw, _) = self.render_handles(monitor_index);

        let mut cursor_offset = 0;
        for ColoredText { text, fg, bg } in texts {
            let width = self.xft.string_cursor_offset(&text, &self.font);

            // Background color.
            let color_gc = self.get_color(bg);
            let rect = (draw, color_gc, cursor_offset, 0, width, self.height);
            self.setup.fill_rects(&[rect]);

            // Foreground text.
            let fg = self.xft.create_color(*fg);
            self.xft.draw_string(
                &text,
                &text_draw,
                &fg,
                &self.font,
                self.height as u32,
                cursor_offset,
            );
            cursor_offset += width;
        }
    }

    fn render_string_right(&self, monitor_index: usize, texts: &[ColoredText]) {
        let (draw, text_draw, monitor_width) = self.render_handles(monitor_index);

        let mut text_width = 0;
        let text_widths = texts
            .iter()
            .map(|text| {
                let cursor_offset = self.xft.string_cursor_offset(&text.text, &self.font);
                text_width += cursor_offset;
                cursor_offset
            })
            .collect::<Vec<_>>();

        let mut cursor_offset = monitor_width - text_width;
        for (ColoredText { text, fg, bg }, width) in texts.iter().zip(text_widths.into_iter()) {
            // Background color.
            let color_gc = self.get_color(bg);
            let rect = (draw, color_gc, cursor_offset, 0, width, self.height);
            self.setup.fill_rects(&[rect]);

            // Foreground text.
            let fg = self.xft.create_color(*fg);
            self.xft.draw_string(
                text,
                &text_draw,
                &fg,
                &self.font,
                self.height,
                cursor_offset,
            );
            cursor_offset += width;
        }
    }

    pub fn render_string(
        &mut self,
        monitor_index: usize,
        alignment: Alignment,
        texts: &[ColoredText],
    ) {
        self.cache_colors(monitor_index, texts);
        match alignment {
            Alignment::Left => self.render_string_left(monitor_index, texts),
            Alignment::Right => self.render_string_right(monitor_index, texts),
        }
    }

    fn blit(&self) {
        self.setup.copy_areas(
            &self
                .monitors
                .iter()
                .map(|monitor| {
                    (
                        monitor.pixmap,
                        monitor.window,
                        self.clear_gc,
                        monitor.w,
                        self.height,
                    )
                })
                .collect::<Vec<_>>(),
        );
    }
}

fn render(bar: &mut Bar) {
    let red = (65535, 0, 0, 65535);
    let blue = (0, 0, 65535, 65535);
    let black = (0, 0, 0, 65535);
    let white = (65535, 65535, 65535, 65535);
    let green = (0, 65535, 0, 65535);

    bar.clear_monitors();
    bar.render_string(
        0,
        Alignment::Left,
        &[
            ColoredText {
                text: "".to_owned(),
                fg: white,
                bg: red,
            },
            ColoredText {
                text: "t s g g s y j p g a g         ".to_owned(),
                fg: red,
                bg: white,
            },
            ColoredText {
                text: "".to_owned(),
                fg: white,
                bg: red,
            },
            ColoredText {
                text: "leftlast1".to_owned(),
                fg: black,
                bg: blue,
            },
        ],
    );

    bar.render_string(
        0,
        Alignment::Right,
        &[
            ColoredText {
                text: "rightfirst1".to_owned(),
                fg: green,
                bg: red,
            },
            ColoredText {
                text: "rightlast1".to_owned(),
                fg: white,
                bg: black,
            },
        ],
    );

    bar.render_string(
        1,
        Alignment::Left,
        &[
            ColoredText {
                text: "tsggsyjpgagOQIWUOEIRJSLKN<VMCXNV".to_owned(),
                fg: red,
                bg: white,
            },
            ColoredText {
                text: "white black".to_owned(),
                fg: white,
                bg: black,
            },
            ColoredText {
                text: "white red".to_owned(),
                fg: white,
                bg: red,
            },
            ColoredText {
                text: "white blue".to_owned(),
                fg: white,
                bg: blue,
            },
            ColoredText {
                text: "white green".to_owned(),
                fg: white,
                bg: green,
            },
        ],
    );

    bar.render_string(
        1,
        Alignment::Right,
        &[
            ColoredText {
                text: "          ".to_owned(),
                fg: white,
                bg: red,
            },
            ColoredText {
                text: "".to_owned(),
                fg: green,
                bg: white,
            },
        ],
    );
}

fn main() {
    // TODO handle signals.
    // TODO Use execution path: arg0.
    // TODO Handle ARGS
    // TODO clickable areas.

    // Connect to the Xserver and initialize scr
    let mut bar = Bar::new();

    render(&mut bar);
    bar.blit();
    bar.setup.flush();

    loop {
        let mut redraw = false;

        render(&mut bar);
        redraw = true;

        if redraw {
            bar.blit();
        }
        bar.setup.flush();
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}
