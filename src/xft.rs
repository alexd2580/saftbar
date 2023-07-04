use std::{collections::HashMap, ffi::c_uchar, ops::DerefMut, ptr::null_mut};

use x11::{xft, xlib, xrender};

pub type RGBA = (u16, u16, u16, u16);

/// Smart object for serverside allocated `XftColor`s.
struct Color {
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
struct Font {
    font: *mut xft::XftFont,
    display: *mut xlib::Display,
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe { xft::XftFontClose(self.display, self.font) };
    }
}

/// Smart object for `XftDraw` pointers.
pub struct Draw {
    pub draw: *mut xft::XftDraw,
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

    /// Cache of available colors.
    colors: HashMap<RGBA, Color>,

    /// Cache of available fonts.
    fonts: HashMap<Vec<u8>, Font>,
}

impl Xft {
    pub fn new(display: *mut xlib::Display, visual: *mut xlib::Visual, colormap_id: u64) -> Self {
        Self {
            display,
            visual,
            colormap_id,
            colors: HashMap::new(),
            fonts: HashMap::new(),
        }
    }

    /// Create a color object, wrap it into a smart object and store.
    pub fn color(&mut self, rgba: RGBA) -> *mut xft::XftColor {
        if let Some(color) = self.colors.get(&rgba) {
            return color.color_ptr;
        }

        let mut render_color = xrender::XRenderColor {
            red: rgba.0,
            green: rgba.1,
            blue: rgba.2,
            alpha: rgba.3,
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
                &mut render_color as *mut xrender::XRenderColor,
                &mut color as *mut xft::XftColor,
            )
        };
        if result == 0 {
            panic!("Failed to create Xft color");
        }

        let mut color = Box::new(color);
        let color_ptr = color.deref_mut() as *mut xft::XftColor;
        self.colors.insert(
            rgba,
            Color {
                color,
                color_ptr,
                display,
                visual,
                colormap_id,
            },
        );
        color_ptr
    }

    /// Load a font by pattern, wrap it into a smart object and store.
    pub fn font(&mut self, font_pattern: &[u8]) -> *mut xft::XftFont {
        let font_pattern = font_pattern.to_owned();
        {
            if let Some(font) = self.fonts.get(&font_pattern) {
                return font.font;
            }
        }

        let display = self.display;
        let pattern_ptr = font_pattern.as_ptr() as *const i8;
        let font = unsafe { xft::XftFontOpenName(display, 0, pattern_ptr) };
        if font == null_mut() {
            panic!("Failed to create Xft font");
        }

        self.fonts.insert(font_pattern, Font { font, display });
        font
    }

    pub fn new_draw(&self, pixmap_id: u64) -> Draw {
        let draw =
            unsafe { xft::XftDrawCreate(self.display, pixmap_id, self.visual, self.colormap_id) };
        if draw == null_mut() {
            panic!("Failed to create Xft draw");
        }

        Draw { draw }
    }

    pub fn draw_string(
        &mut self,
        text: &str,
        draw: *mut xft::XftDraw,
        color: *mut xft::XftColor,
        font: *mut xft::XftFont,
    ) {
        unsafe {
            xft::XftDrawString8(
                draw,
                color,
                font,
                0,
                10, // TODO
                text.as_ptr() as *const c_uchar,
                text.len() as i32,
            )
        };
    }
}
