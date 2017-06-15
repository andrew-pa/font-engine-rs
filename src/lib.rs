
#[macro_use]
extern crate bitflags;
extern crate byteorder;
mod truetype_loader;

struct Point {
    x: i32, y: i32, on_curve: bool
}

struct Glyph {
    contours: Vec<Vec<Point>>
}

struct Font {
    glyphs: Vec<Glyph>
}

/* 
 # ROADMAP #
  + Load TTF files
  - Convert them into some kind of universal internal representation
  - Provide a rasterizer that takes a font, glyph index -> bitmap
  - Provide some kind of layout engine that takes chars or strings -> glyph indices + positions
       for rendering
 */
