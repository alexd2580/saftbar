use std::collections::HashMap;

use log::debug;
use tokio::io::unix::AsyncFd;
use xcb::{x, Xid};

use crate::setup::{ChangeProperty, CopyArea, FillRect, PropertyData, Rectangle, Setup};
use crate::xft::{Draw, Font, Xft, RGBA};

struct Monitor {
    x: u32,
    // y: u32,
    w: u32,

    // Note the reverse drop order! Children first.
    pixmap: x::Pixmap,
    window: xcb::x::Window,
}

#[derive(Copy, Clone)]
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

    // Note the reverse drop order! Children first.
    color_gcs: HashMap<RGBA, x::Gcontext>,
    clear_gc: x::Gcontext,
    font: Font,
    xft: Xft,
    monitors: Vec<Monitor>,
    setup: Setup,
}

impl Bar {
    pub fn new() -> Self {
        let setup = Setup::new();
        let valid_regions = setup.query_valid_crtc_regions();
        let mut xft = setup.create_xft();

        // Use the `Propo` variant to get full size icons, while sacrificing monospace.
        let font_family = "Ubuntu Mono Nerd Font Propo";
        let font = xft.create_font(font_family, 15.25);
        debug!("Loaded font: {font:#?}");

        debug!("Creating windows");
        let height = font.asc_and_desc();
        let monitors = valid_regions
            .into_iter()
            .map(|Rectangle { x, y, w, .. }| {
                let (window, pixmap) =
                    setup.create_window_and_pixmap(x, y, w, height, setup.colormap);

                Monitor {
                    x,
                    // y,
                    w,
                    pixmap,
                    window,
                }
            })
            .collect::<Vec<_>>();

        // Set EWMH or something values.
        debug!("Setting EWMH or something atoms");
        {
            use PropertyData::{Atom, Cardinal, String};

            // Create atoms.
            let [desktop, window_type, window_type_dock, state, state_sticky, strut_partial, strut] =
                setup.get_atoms(&[
                    "_NET_WM_DESKTOP",
                    "_NET_WM_WINDOW_TYPE",
                    "_NET_WM_WINDOW_TYPE_DOCK",
                    "_NET_WM_STATE",
                    "_NET_WM_STATE_STICKY",
                    "_NET_WM_STRUT_PARTIAL",
                    "_NET_WM_STRUT",
                ]);

            let window_type_dock = [window_type_dock];
            let state_sticky = [state_sticky];
            let name_bytes = "saftbar".as_bytes();
            let properties = [
                ChangeProperty(desktop, Cardinal(&[u32::MAX])),
                ChangeProperty(window_type, Atom(&window_type_dock)),
                ChangeProperty(state, Atom(&state_sticky)),
                ChangeProperty(x::ATOM_WM_NAME, String(name_bytes)),
                ChangeProperty(x::ATOM_WM_CLASS, String(name_bytes)),
            ];

            // Set window properties.
            for monitor in &monitors {
                setup.replace_properties(monitor.window, &properties);

                let h = height;
                let sx = monitor.x;
                let ex = sx + monitor.w;
                let strut_data = [0, 0, h, 0, 0, 0, 0, 0, sx, ex, 0, 0];
                let monitor_properties = [
                    ChangeProperty(strut_partial, Cardinal(&strut_data)),
                    ChangeProperty(strut, Cardinal(&strut_data[..4])),
                ];
                setup.replace_properties(monitor.window, &monitor_properties);
            }
        }

        // This is needed to get the root/depth. The root window has 24bpp, we want 32. Why?
        let reference_drawable = x::Drawable::Window(monitors[0].window);
        let clear_gc = setup.create_gc(reference_drawable, &[x::Gc::Foreground(0x0000_0000)]);

        // Make windows visible.
        debug!("Mapping windows");
        setup.map_windows(
            &monitors
                .iter()
                .map(|monitor| crate::setup::MapWindow(monitor.window))
                .collect::<Vec<_>>(),
        );

        setup.flush();
        debug!("Bar initialization done");

        // TODO handle signals.
        // TODO Use execution path: arg0.
        // TODO clickable areas.

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

    fn cache_color(&mut self, reference_drawable: x::Drawable, rgba: RGBA) {
        if self.color_gcs.get(&rgba).is_none() {
            let r = u32::from(rgba.0);
            let g = u32::from(rgba.1);
            let b = u32::from(rgba.2);
            let a = u32::from(rgba.3);
            let color = b | g << 8 | r << 16 | a << 24;

            let gc = self
                .setup
                .create_gc(reference_drawable, &[x::Gc::Foreground(color)]);

            self.color_gcs.insert(rgba, gc);
        }
    }

    fn get_color(&self, rgba: RGBA) -> x::Gcontext {
        self.color_gcs
            .get(&rgba)
            .copied()
            .expect("Color is not cached")
    }

    pub fn clear_monitors(&self) {
        self.setup.fill_rects(
            &self
                .monitors
                .iter()
                .map(|monitor| {
                    FillRect(
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
        for text in texts {
            self.cache_color(drawable, text.bg);
        }
    }

    fn render_handles(&self, monitor_index: usize) -> (x::Drawable, Draw, u32) {
        let monitor = &self.monitors[monitor_index];
        let pixmap = monitor.pixmap;
        (
            x::Drawable::Pixmap(pixmap),
            self.xft.new_draw(u64::from(pixmap.resource_id())),
            monitor.w,
        )
    }

    fn render_string_left(&self, monitor_index: usize, texts: &[ColoredText]) {
        let (draw, text_draw, _) = self.render_handles(monitor_index);

        let mut cursor_offset = 0;
        for ColoredText { text, fg, bg } in texts {
            let width = self.xft.string_cursor_offset(text, &self.font);

            // Background color.
            let color_gc = self.get_color(*bg);
            let rect = FillRect(draw, color_gc, cursor_offset, 0, width, self.height);
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
            let color_gc = self.get_color(*bg);
            let rect = FillRect(draw, color_gc, cursor_offset, 0, width, self.height);
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

    pub fn blit(&self) {
        self.setup.copy_areas(
            &self
                .monitors
                .iter()
                .map(|monitor| {
                    CopyArea(
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

    pub fn flush(&self) {
        self.setup.flush();
    }

    pub async fn next_x_event(&self) -> xcb::Event {
        loop {
            if let Some(event) = self.setup.poll_for_event() {
                return event;
            }

            let async_fd = AsyncFd::new(self.setup.raw_connection_fd())
                .expect("Failed to initialize async fd");
            // Drop the guard immediately. We are only interested in noticing action on the
            // file descriptor.
            let _ = async_fd
                .readable()
                .await
                .expect("Failed to wait for events");
        }
    }
}

impl Default for Bar {
    fn default() -> Self {
        Self::new()
    }
}
