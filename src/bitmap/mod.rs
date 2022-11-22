use std::collections::HashMap;

use image::{RgbImage, EncodableLayout};

use super::{
    BitmapBuffer, Error, FontDesc, FontKey, GlyphKey, Metrics, RasterizedGlyph, Size,
};

// The first visable character is the '!', which is at index 33 in unicode, but index 1
// in the glyph sheet. (Index 0 is reserved for metrics information).
const FIRST_CHARACTER: usize = 33;

// Pixels are loaded as RGB.
const PIXEL_COMPONENTS: usize = 3;

struct BitmapFont {
    img: RgbImage,
    atlas_width: usize,
    padding_width: usize,
    average_advance: usize,
    line_height: usize,
    underline_position: usize,
    underline_thickness: usize,
    strikeout_position: usize,
    strikeout_thickness: usize,
}

pub struct BitmapRasterizer {
    fonts: HashMap<FontKey, BitmapFont>,
    keys: HashMap<FontDesc, FontKey>,
}

impl BitmapRasterizer {
    fn rasterize_glyph(
        &self,
        glyph: GlyphKey,
    ) -> Result<RasterizedGlyph, Error> {
        let character = glyph.character;
        let character_index = character as usize;

        if character_index < FIRST_CHARACTER {
            return Err(Error::UnknownFontKey)
        }

        let loaded_font = self.get_loaded_font(glyph.font_key)?;

        let buffer = {
            let mut data = Vec::<u8>::new();
            let font_data = loaded_font.img.as_bytes();

            let x_offset = (loaded_font.average_advance + loaded_font.padding_width) * (character_index - FIRST_CHARACTER + 1);

            for y in 0..loaded_font.line_height {
                for x in 0..loaded_font.average_advance {
                    let i = (x + x_offset + y * loaded_font.atlas_width) * PIXEL_COMPONENTS;
                    data.push(font_data[i]);
                    data.push(font_data[i + 1]);
                    data.push(font_data[i + 2]);
                }
            }

            BitmapBuffer::Rgb(data)
        };

        Ok(RasterizedGlyph {
            character,
            width: loaded_font.average_advance as i32,
            height: loaded_font.line_height as i32,
            top: loaded_font.line_height as i32,
            left: 0,
            advance: (0, 0),
            buffer,
        })
    }

    fn get_loaded_font(&self, font_key: FontKey) -> Result<&BitmapFont, Error> {
        self.fonts.get(&font_key).ok_or(Error::UnknownFontKey)
    }
}

impl crate::Rasterize for BitmapRasterizer {
    fn new(_device_pixel_ratio: f32) -> Result<BitmapRasterizer, Error> {
        Ok(BitmapRasterizer {
            fonts: HashMap::new(),
            keys: HashMap::new(),
        })
    }

    fn metrics(&self, key: FontKey, _size: Size) -> Result<Metrics, Error> {
        let loaded_font = self.get_loaded_font(key)?;

        Ok(Metrics {
            descent: 0.0,
            average_advance: loaded_font.average_advance as f64,
            line_height: loaded_font.line_height as f64,
            underline_position: loaded_font.underline_position as f32,
            underline_thickness: loaded_font.underline_thickness as f32,
            strikeout_position: loaded_font.strikeout_position as f32,
            strikeout_thickness: loaded_font.strikeout_thickness as f32,
        })
    }

    fn load_font(&mut self, desc: &FontDesc, _size: Size) -> Result<FontKey, Error> {
        let font_file = match image::io::Reader::open(&desc.name) {
            Ok(file) => file,
            Err(_) => return Err(Error::FontNotFound(desc.clone())),
        };

        let font_img = match font_file.decode() {
            Ok(img) => img.into_rgb8(),
            Err(_) => return Err(Error::PlatformError("Failed to decode font".into())),
        };

        let font_atlas_width = font_img.width() as usize;
        let font_atlas_height = font_img.height() as usize;

        let font_data = font_img.as_bytes();

        let mut average_advance = 0;
        for x in 0..(font_atlas_width as usize) {
            if check_pixel_color(font_data, font_atlas_width, x, 0, 0, 255, 0) {
                average_advance = x + 1;
                break;
            }
        }

        if average_advance == 0 {
            return Err(Error::PlatformError("Can't determine font glyph width".into()));
        }

        let mut padding_width = 0;
        for x in average_advance..font_atlas_width {
            if !check_pixel_color(font_data, font_atlas_width, x, 0, 255, 0, 255) {
                break;
            }

            padding_width += 1;
        }

        let mut underline_position = 0;
        let mut underline_thickness = 0;
        for y in 0..(font_atlas_height as usize) {
            if check_pixel_color(font_data, font_atlas_width, 0, y, 255, 0, 0) {
                if underline_position == 0 {
                    underline_position = font_atlas_height - y;
                }

                underline_thickness += 1;
            }
        }

        if underline_position == 0 {
            return Err(Error::PlatformError("Can't determine font underline position".into()));
        }

        let mut strikeout_position = 0;
        let mut strikeout_thickness = 0;
        for y in 0..(font_atlas_height as usize) {
            if check_pixel_color(font_data, font_atlas_width, 0, y, 0, 0, 255) {
                if strikeout_position == 0 {
                    strikeout_position = font_atlas_height - y;
                }

                strikeout_thickness += 1;
            }
        }

        if strikeout_position == 0 {
            return Err(Error::PlatformError("Can't determine font strikeout position".into()));
        }

        let key = FontKey::next();
        self.keys.insert(desc.clone(), key);
        self.fonts.insert(key, BitmapFont {
            img: font_img,
            atlas_width: font_atlas_width,
            line_height: font_atlas_height,
            padding_width,
            average_advance,
            underline_position,
            underline_thickness,
            strikeout_position,
            strikeout_thickness,
        });

        Ok(key)
    }

    fn get_glyph(&mut self, glyph: GlyphKey) -> Result<RasterizedGlyph, Error> {
        let rasterized_glyph =
            self.rasterize_glyph(glyph)?;

        Ok(rasterized_glyph)
    }

    fn kerning(&mut self, _left: GlyphKey, _right: GlyphKey) -> (f32, f32) {
        (0., 0.)
    }

    fn update_dpr(&mut self, _device_pixel_ratio: f32) {
    }
}

fn check_pixel_color(data: &[u8], atlas_width: usize, x: usize, y: usize, r: u8, g: u8, b: u8) -> bool {
    let i  = (x + y * atlas_width) * PIXEL_COMPONENTS as usize;

    data[i] == r && data[i + 1] == g && data[i + 2] == b
}
