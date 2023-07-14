use std::ops::DerefMut;

use x11::{xft, xlib, xrender};

pub type RGBA = (u8, u8, u8, u8);

/// Smart object for serverside allocated `XftColor`s.
pub struct Color {
    color: Box<xft::XftColor>,
    color_ptr: *mut xft::XftColor,
    display: *mut xlib::Display,
    visual: *mut xlib::Visual,
    colormap_id: u64,
}

impl Drop for Color {
    fn drop(&mut self) {
        unsafe { xft::XftColorFree(self.display, self.visual, self.colormap_id, self.color_ptr) };
    }
}

/// Smart object for `XftFont` pointers.
#[derive(Debug)]
pub struct Font {
    font: *mut xft::XftFont,
    ascent: u32,
    descent: u32,
    display: *mut xlib::Display,
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe { xft::XftFontClose(self.display, self.font) };
    }
}

/// Smart object for `XftDraw` pointers.
pub struct Draw {
    draw: *mut xft::XftDraw,
}

impl Drop for Draw {
    fn drop(&mut self) {
        unsafe { xft::XftDrawDestroy(self.draw) };
    }
}

/// State machine holding the resources for rendering text.
pub struct Xft {
    display: *mut xlib::Display,
    visual: *mut xlib::Visual,
    colormap_id: u64,
}

impl Xft {
    pub fn new(display: *mut xlib::Display, visual: *mut xlib::Visual, colormap_id: u64) -> Self {
        Self {
            display,
            visual,
            colormap_id,
        }
    }

    /// Create a color object, wrap it into a smart object and store.
    #[must_use]
    pub fn create_color(&self, rgba: RGBA) -> Color {
        let mut render_color = xrender::XRenderColor {
            red: u16::from(rgba.0) << 8,
            green: u16::from(rgba.1) << 8,
            blue: u16::from(rgba.2) << 8,
            alpha: u16::from(rgba.3) << 8,
        };

        let mut color = xft::XftColor {
            pixel: 0,
            color: xrender::XRenderColor {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
        };

        let display = self.display;
        let visual = self.visual;
        let colormap_id = self.colormap_id;

        let result = unsafe {
            xft::XftColorAllocValue(
                display,
                visual,
                colormap_id,
                std::ptr::addr_of_mut!(render_color),
                std::ptr::addr_of_mut!(color),
            )
        };
        assert_ne!(result, 0, "Failed to create Xft color");

        let mut color = Box::new(color);
        let color_ptr = std::ptr::addr_of_mut!(*color.deref_mut());
        Color {
            color,
            color_ptr,
            display,
            visual,
            colormap_id,
        }
    }

    /// Load a font by pattern, wrap it into a smart object and store.
    /// Trailing 0 required for `font_pattern`!
    pub fn create_font(&mut self, font_pattern: &str) -> Font {
        let display = self.display;
        let pattern_ptr = font_pattern.as_ptr().cast::<i8>();
        let font = unsafe { xft::XftFontOpenName(display, 0, pattern_ptr) };
        let (ascent, descent) = unsafe {
            (
                (*font).ascent.try_into().unwrap(),
                (*font).descent.try_into().unwrap(),
            )
        };
        assert!(!font.is_null(), "Failed to create Xft font");

        Font {
            font,
            ascent,
            descent,
            display,
        }
    }

    #[must_use]
    pub fn new_draw(&self, pixmap_id: u64) -> Draw {
        let draw =
            unsafe { xft::XftDrawCreate(self.display, pixmap_id, self.visual, self.colormap_id) };
        assert!(!draw.is_null(), "Failed to create Xft draw");

        Draw { draw }
    }

    #[must_use]
    pub fn string_cursor_offset(&self, text: &str, font: &Font) -> u32 {
        let text_ptr = text.as_ptr();
        let text_len = i32::try_from(text.len()).unwrap();
        let mut extents = xrender::XGlyphInfo {
            width: 0,
            height: 0,
            x: 0,
            y: 0,
            xOff: 0,
            yOff: 0,
        };
        unsafe {
            let extents_ptr = std::ptr::addr_of_mut!(extents);
            xft::XftTextExtentsUtf8(self.display, font.font, text_ptr, text_len, extents_ptr);
            extents.xOff.try_into().unwrap()
        }
    }

    pub fn draw_string(
        &self,
        text: &str,
        draw: &Draw,
        color: &Color,
        font: &Font,
        canvas_height: u32,
        cursor_offset: u32,
    ) {
        let text_ptr = text.as_ptr();
        let text_len = i32::try_from(text.len()).unwrap();
        let baseline_offset = (canvas_height + font.ascent - font.descent) / 2;
        unsafe {
            xft::XftDrawStringUtf8(
                draw.draw,
                color.color_ptr,
                font.font,
                cursor_offset.try_into().unwrap(),
                baseline_offset.try_into().unwrap(),
                text_ptr,
                text_len,
            );
        }
    }
}
