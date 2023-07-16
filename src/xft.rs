use std::ops::DerefMut;

use x11::{xft, xlib, xrender};

pub type RGBA = (u8, u8, u8, u8);

/// Smart object for serverside allocated `XftColor`s.
pub struct Color {
    /// Xft color object. Used as a pointer, therefore the object itself is never accessed.
    #[allow(dead_code)]
    color: Box<xft::XftColor>,

    /// Pointer to the color object. This is a cached value for convenience.
    color_ptr: *mut xft::XftColor,

    /// Render reference resources.
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
    #[allow(dead_code)]
    descent: u32,
    display: *mut xlib::Display,
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe { xft::XftFontClose(self.display, self.font) };
    }
}

impl Font {
    #[must_use]
    pub fn asc_and_desc(&self) -> u32 {
        self.ascent + self.descent
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
    ///
    /// # Panics
    ///
    /// This function expects `XftColorAllocValue` to not fail.
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
        assert_ne!(result, 0, "Xft color creation failed");

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
    ///
    /// # Sizes
    ///
    /// It is ESSENTIAL to choose the right font size. Note that `size` is a double. This is not a
    /// mistake. This is by design. Fonts are rasterized around a baseline. When specifying a pixel
    /// size you are specifying the non-grid-aligned vertical height of a font, not its actual
    /// pixel height, which will most likely be one pixel more than the specified height.
    ///
    /// Try out different fractional heights until you find a symmetric one for the font that you
    /// want to use.
    ///
    /// # Panics
    ///
    /// This function expects `XftFontLoad` to not fail and the loaded font to have sensible
    /// values, especially positive ascent and descent.
    pub fn create_font(&mut self, font_family: &str, size: f32) -> Font {
        let display = self.display;
        let font_pattern = format!(
            "{font_family}:size={size:.5}:antialias=true:hinting=true:hintstyle=hintnone\0"
        );
        let pattern_ptr = font_pattern.as_ptr().cast::<i8>();
        let font = unsafe { xft::XftFontOpenName(display, 0, pattern_ptr) };
        assert!(!font.is_null(), "Xft font creation failed");
        let x_font = &unsafe { *font };
        let (ascent, descent) = (
            x_font.ascent.try_into().expect("Font ascent is negative"),
            x_font.descent.try_into().expect("Font descent is negative"),
        );

        Font {
            font,
            ascent,
            descent,
            display,
        }
    }

    /// Create a `Draw` - a temporary object holding references to the drawable and the context.
    ///
    /// # Panics
    ///
    /// This function expects `XftDrawCreate` to not fail.
    #[must_use]
    pub fn new_draw(&self, pixmap_id: u64) -> Draw {
        let draw =
            unsafe { xft::XftDrawCreate(self.display, pixmap_id, self.visual, self.colormap_id) };
        assert!(!draw.is_null(), "Failed to create Xft draw");

        Draw { draw }
    }

    fn c_text_ptr_len(text: &str) -> (*const u8, i32) {
        (
            text.as_ptr(),
            text.len().try_into().expect("Text is longer than i16::MAX"),
        )
    }

    #[must_use]
    pub fn string_cursor_offset(&self, text: &str, font: &Font) -> u32 {
        let (text_ptr, text_len) = Self::c_text_ptr_len(text);
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
            extents
                .xOff
                .try_into()
                .expect("Cursor offset is (probably) a negative value")
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
        let (text_ptr, text_len) = Self::c_text_ptr_len(text);
        // WTF... if was right here all the time. If my canvas has the same size as the font then i
        // don't need no centering stuff, the ascent IS the baseline offset.
        // The major problem is choosing a font size so that its (!) rasterization is vertically
        // symmetric! See screenshot for reference.
        // let baseline_offset = font.ascent;

        // If the canvas is larger than asc+desc then we hope that the overhang is an even number
        // of pixels. Otherwise we're off by 0.5 pixels.
        let baseline_offset = (canvas_height - font.asc_and_desc()) / 2 + font.ascent;
        unsafe {
            xft::XftDrawStringUtf8(
                draw.draw,
                color.color_ptr,
                font.font,
                cursor_offset
                    .try_into()
                    .expect("Cursor offset not representable as c_int"),
                baseline_offset
                    .try_into()
                    .expect("Baseline offset not representable as c_int"),
                text_ptr,
                text_len,
            );
        }
    }
}
