#![feature(slice_patterns)]

#[macro_use]
extern crate bitflags;
extern crate byteorder;
extern crate fix;
extern crate typenum;
mod truetype_loader;
mod numerics;
mod interp_instructor;

use std::error::Error;

/* 
 # ROADMAP #
  + Load TTF files
  - Convert them into some kind of universal internal representation
  - Provide a rasterizer that takes a font, glyph index -> bitmap
  - Provide some kind of layout engine that takes chars or strings -> glyph indices + positions
       for rendering
 */


#[derive(Copy,Clone,Debug)]
pub struct Point {
    x: f32, y: f32
}

impl Point {
    fn new(x: f32, y: f32) -> Point {
        Point { x, y }
    }
}

#[derive(Copy,Clone,Debug)]
pub enum Curve {
    Line(usize,usize),
    Quad(usize,usize,usize) // (start, ctrl, end)
}
#[derive(Debug)]
pub struct Glyph {
    curves: Vec<Curve>,
    points: Vec<Point>
}

struct CharMap<'fontdata> {
    id_map: &'fontdata [u8; 256]
}

impl<'fontdata> CharMap<'fontdata> {
    fn from_truetype<'f>(font: &'f truetype_loader::SfntFont) -> CharMap<'f> {
        CharMap {
            id_map: font.cmap_table.as_ref().and_then(|table| {
                for enc_tbl in &table.encoding_tables {
                    match &enc_tbl.subtable {
                        &truetype_loader::CharGlyphMappingEncodingTableFormat::ByteEncoding { glyph_ids: ref ids } => { return Some(ids) },
                        _ => {}
                    }
                }
                None
            }).expect("font has format 0 table")
        }
    }
    fn map(&self, c: char) -> usize {
        let ci = c as u32;
        if ci > 256 { return 0 }
        self.id_map[ci as usize] as usize
    }
}

fn wrap(i: usize, start: usize, end: usize) -> usize {
    let len = end-start;
    if i >= end {
        start + ((i-start) % len)
    } else if i < start {
        start + ((start-i) % len)
    } else { i }
}

impl Glyph {
    pub fn from_truetype_with_points(ttf_glyph: &truetype_loader::GlyphDescription, cpoints: Vec<Point>) -> Option<Glyph> {
        use truetype_loader::*;
        //println!("{:?}", glyph);
        match ttf_glyph {
            &GlyphDescription::Simple { 
                num_contours, x_min, x_max, y_min, y_max, ref end_points_of_contours, ref instructions, points: ref spoints 
            } => {
                let mut curves = Vec::new();
                let mut points = cpoints.clone();
                //    .map(|&GlyphPoint { x, y, .. }| Point { x: x as f32, y: y as f32 }).collect::<Vec<_>>();
                let mut last_endpoint = 0;
                for &ep in end_points_of_contours {
                    let endpoint = ep as usize + 1;
                    let mut i = last_endpoint;
                    while !spoints[i].on_curve { i += 1 }
                    let mut last_point = i; i += 1;
                    while i < endpoint as usize {
                        if spoints[i].on_curve {
                            curves.push(Curve::Line(last_point as usize, i as usize));
                            println!("{} {}", last_point, i);
                            last_point = i;
                            i += 1;
                        } else {
                            let mut a = points[i]; let mut ia = i;
                            i += 1; let mut ib = wrap(i, last_endpoint, endpoint);
                            let mut b = points[ib];
                            if spoints[ib].on_curve {
                                curves.push(Curve::Quad(last_point, ia, ib));
                                last_point = ib;
                            } else {
                                while !spoints[ib].on_curve {
                                    let midx = (a.x + b.x) / 2.0;
                                    let midy = (a.y + b.y) / 2.0;
                                    let im = points.len();
                                    points.push(Point { x: midx as f32, y: midy as f32 });
                                    curves.push(Curve::Quad(last_point, ia, im)); last_point = im;
                                    a = b; ia = ib;
                                    i += 1; ib = wrap(i,last_endpoint,endpoint);
                                    b = points[ib];
                                }
                                curves.push(Curve::Quad(last_point, ia, ib)); last_point = ib;
                            }
                        }
                    }
                    curves.push(Curve::Line(last_point, last_endpoint));
                    last_endpoint = endpoint;
                }
                Some(Glyph { curves, points })
            },
            &GlyphDescription::Composite { .. } => None,
            &GlyphDescription::None => None
        }
    }

    pub fn from_truetype(ttf_glyph: &truetype_loader::GlyphDescription) -> Option<Glyph> {
        match ttf_glyph {
            &truetype_loader::GlyphDescription::Simple { ref points, .. } =>
                Glyph::from_truetype_with_points(ttf_glyph, points.iter().map(|&truetype_loader::GlyphPoint { x, y, .. }| Point { x: x as f32, y: y as f32 }).collect()),
            _ => None
        }
    }
}

pub trait GlyphScaler {
    fn uniform_scale(&self, point_size: f32) -> f32;
    fn scale_glyph(&self, point_size: f32, glyph_index: usize, offset: Point) -> Result<Glyph, Box<Error>>;
}

pub struct SimpleGlyphScaler<'f> {
    glyph_table: &'f truetype_loader::GlyphDataTable,
    output_dpi: f32,
    units_per_em: f32
}

impl<'f> SimpleGlyphScaler<'f> {
    fn new(font: &'f truetype_loader::SfntFont, dpi: f32) -> Result<SimpleGlyphScaler<'f>, Box<Error>> {
        Ok(SimpleGlyphScaler {
            output_dpi: dpi,
            units_per_em: font.head_table.ok_or("font missnig head table")?.units_per_em as f32,
            glyph_table: font.glyf_table.as_ref().ok_or("font missing glyph table")?
        })
    }
}

impl<'f> GlyphScaler for SimpleGlyphScaler<'f> {
    fn uniform_scale(&self, point_size: f32) -> f32 {
        point_size * self.output_dpi / (72f32 * self.units_per_em)
    }
    fn scale_glyph(&self, point_size: f32, glyph_index: usize, offset: Point) -> Result<Glyph, Box<Error>> {
        let scale = self.uniform_scale(point_size);
        let mut g = Glyph::from_truetype(&self.glyph_table.glyphs[glyph_index]).ok_or("glyph from truetype")?;
        for p in g.points.iter_mut() {
            p.x = p.x * scale + 8.0 + offset.x; 
            p.y = (self.units_per_em-p.y) * scale + offset.y;
        }
        Ok(g)
    }
}

pub struct Rasterizer<S: GlyphScaler> {
    scaler: S
}

fn inside<T: PartialOrd>(x: T, min: T, max: T) -> bool {
    let (rmin, rmax) = if min > max { (max, min) } else { (min, max) };
    assert!(rmin <= rmax);
    x >= rmin && x <= rmax
}

impl Curve {
    
    // intersects this curve with a test ray that goes along the +Y direction from the point (x,y)
    fn intersects_test_ray(&self, points: &Vec<Point>, tx: f32, y: f32) -> bool {
        match self {
            &Curve::Line(start, end) => {
                // y-y1 = m(x-x1)
                // y = $y; is there an x value that satisfies? x = (y-y1)/m + x1 
                // x must be less than end.x and greater than start.x
                // wait: what if m = 0 or m = +/- inf? line reduces to a basic interval
                let m = (points[end].y - points[start].y) / (points[end].x - points[start].x);
                //println!("tx={}, ty={}, points[start]={:?}, points[end]={:?}, X{}, Y{}", tx, y, points[start], points[end], inside(tx, points[start].x, points[end].x), inside(y, points[start].y, points[end].y));
                if m == 0f32 {
                    // y-y1 = 0(x-x1)
                    // change in y = 0, so only X point matters
                    //println!("tx={}, ty={}, points[start]={:?}, points[end]={:?}, X{}, Y{}", tx, y, points[start], points[end], inside(tx, points[start].x, points[end].x), inside(y, points[start].y, points[end].y));
                    //inside(y, points[start].y, points[end].y) 
                    false
                } else if m == std::f32::INFINITY || m == std::f32::NEG_INFINITY {
                    // change in x = 0, so only Y point matters
                    inside(y, points[start].y, points[end].y) && tx <= points[start].x 
                } else {
                    let x = (y - points[start].y)/m + points[start].x;
                    //println!("m={}, x={}", m, x);
                    //x >= tx
                    inside(x, points[start].x, points[end].x) && x >= tx
                }
            },

            &Curve::Quad(start, ctrl, end) => {
                // (x,y) = (1-t)²p₀ + 2*(1-t)*t*p₁ + t²p₂
                // y = $y; there are two t values that satisfy, and the x values can be found using
                // the original equation. If we only wish to check existance, only the determinant
                // matters
                let a = points[start].y; let b = points[ctrl].y; let c = points[end].y;
                let det = -a*c + a*y + b*b - 2f32*b*y + c*y;
                det > 0f32
            }
        }
    }

    fn intersect_scanline(&self, points: &Vec<Point>, y: f32, result: &mut Vec<f32>) {
        match self {
            &Curve::Line(start, end) => {
                let Point{x: startx, y: starty} = points[start];
                let Point{x: endx, y: endy} = points[end];
                if !inside(y, starty, endy) { return; }
                if (startx-endx).abs() < 0.001 {
                    result.push(startx); 
                } else if (starty-endy).abs() < 0.001 {
                    result.push(startx); result.push(endx); 
                } else {
                    let m = (endy-starty)/(endx-startx);
                    result.push((y-starty)/m + startx); 
                }
            },
            &Curve::Quad(start, ctrl, end) => {
                let a = points[start].y; let b = points[ctrl].y; let c = points[end].y;
                let det = -a*c + a*y + b*b - 2.0*b*y + c*y;
                if det < 0.0 { return; }
                let sqrt_det = det.sqrt();
                let denom = -a + 2.0*b - c;
                let t1 = (-sqrt_det - a + b) / denom;
                let t2 = (sqrt_det - a + b) / denom;
                if inside(t1, 0.0, 1.0) {
                    result.push((1.0-t1)*(1.0-t1)*points[start].x + 2.0*(1.0-t1)*t1*points[ctrl].x + t1*t1*points[end].x);
                }
                if inside(t2, 0.0, 1.0) {
                    result.push((1.0-t2)*(1.0-t2)*points[start].x + 2.0*(1.0-t2)*t2*points[ctrl].x + t2*t2*points[end].x);
                }
            }
        }
    }
}

impl<S: GlyphScaler> Rasterizer<S> {
    pub fn scale(&self, point_size: f32) -> f32 {
        self.scaler.uniform_scale(point_size)
        //point_size * self.output_dpi / (72f32 * self.units_per_em)
    }

    pub fn raster_glyph<'a>(&self, glyph_index: usize, bitmap: &'a mut [u8], width: usize, point_size: f32, offset: Point) -> Result<&'a [u8], Box<Error>> {
        let height = bitmap.len() / width;
        //scale & grid fit the outline
        // this involves interpreting some instructions
        let glyph = self.scaler.scale_glyph(point_size, glyph_index, offset)?;
        //rasterize by scan line
        for y in 0..height {
            let mut xs = Vec::new();
            for curve in &glyph.curves {
                curve.intersect_scanline(&glyph.points, y as f32 + 0.5, &mut xs);
            }
            use std::cmp::Ordering;
            xs.sort_unstable_by(|a, b| if *a < *b { Ordering::Less } else { Ordering::Greater });
            for px in xs.chunks(2) {
                if px.len() != 2 { continue; }
                for x in (px[0] as usize)..(px[1] as usize) {
                    bitmap[x + (y as usize)*width] = 255;
                }
            }
        }
        /*for p in points {
            println!("{:?}", p);
            bitmap[(p.x as usize) + (p.y.abs() as usize)*width] = 128;
        }*/
        Ok(bitmap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;
    use std::fs::File;

    extern crate image;
    use self::image::{ImageBuffer,Luma,Pixel};
    extern crate svg;

    use self::svg::{Document,Node};
    use self::svg::node::element::{Text, Path as GPath, Rectangle, Circle, Group, Line};
    use self::svg::node::element::path::Data;

    fn glyph_to_svg(g: &Glyph, scale: f32) -> Document {
        let mut doc = Document::new();
        let mut gr = Group::new();
        for c in &g.curves {
            match c {
                &Curve::Line(start, end) => {
                    let mut c = Data::new();
                    c = c.move_to((g.points[start].x*scale, g.points[start].y*scale));
                    c = c.line_to((g.points[end].x*scale, g.points[end].y*scale));
                    gr.append(GPath::new().set("fill","none").set("stroke","orange").set("stroke-width",6).set("d",c));
                },
                &Curve::Quad(start, ctl, end) => {
                    let mut c = Data::new();
                    c = c.move_to((g.points[start].x*scale, g.points[start].y*scale));
                    c = c.quadratic_curve_to((g.points[ctl].x*scale, g.points[ctl].y*scale, g.points[end].x*scale, g.points[end].y*scale));
                    gr.append(GPath::new().set("fill","none").set("stroke","orangered").set("stroke-width",6).set("d",c));
                }
            }
        }
        for (i,p) in g.points.iter().enumerate() {
            gr.append(Circle::new()
                      .set("cx",p.x*scale)
                      .set("cy",p.y*scale)
                      .set("r",4));
            gr.append(Text::new().set("x",p.x*scale+10f32).set("y",p.y*scale+10f32).add(svg::node::Text::new(format!("{}",i))));
        }


        doc.append(gr);
        doc.assign("viewBox", (0f32, -50f32, scale*2048f32*2f32, scale*2048f32*2f32));

        doc
    }

    const test_glyph_index: usize = 9;
    #[cfg(target_os="windows")]
    const FONT_PATH: &'static str = 
        "C:\\Windows\\Fonts\\arial.ttf";
    #[cfg(target_os="macos")]
    const FONT_PATH: &'static str = 
        "/Library/Fonts/Arial.ttf";

    #[test]
    fn load_truetype_svg_out() {
        use truetype_loader::*;
        let mut font_file = File::open(FONT_PATH).unwrap();
        let font = SfntFont::from_binary(&mut font_file).expect("load font data");

        let g = Glyph::from_truetype(font.glyf_table.as_ref().map(|t| &t.glyphs[test_glyph_index]).expect("load glyph")).unwrap();
        let doc = glyph_to_svg(&g, 0.5f32);
        svg::save("glyph_conv.svg", &doc).unwrap();
    }

    #[test]
    fn load_truetype_read_instructions() {
        use truetype_loader::*;
        let mut font_file = File::open(FONT_PATH).unwrap();
        let font = SfntFont::from_binary(&mut font_file).expect("load font data");

        font.glyf_table.as_ref().map(|glyf_table| match glyf_table.glyphs[test_glyph_index] {
            GlyphDescription::Simple { ref instructions, .. } => {
                for is in instructions.chunks(8) {
                    for i in is {
                        print!("{:2x} ", i);
                    }
                    println!();
                }
            },
            _ => println!("!")
        });
    }



    #[test]
    fn intersect_outlines_svg() {
        use truetype_loader::*;
        let mut font_file = File::open(FONT_PATH).unwrap();
        let font = SfntFont::from_binary(&mut font_file).expect("load font data");
        let g = Glyph::from_truetype(font.glyf_table.as_ref()
                                     .map(|t| &t.glyphs[test_glyph_index]).expect("load glyph")).unwrap();
        let mut doc = glyph_to_svg(&g, 1.0f32);
        for iy in (0u32..90u32) {
            let y = (iy as f32) * 32.0;
            let mut ipoints = Vec::new();
            for curve in g.curves.iter() {
                curve.intersect_scanline(&g.points, y, &mut ipoints);
            }
            doc.append(Line::new().set("x1", 0).set("y1", y)
                       .set("x2", 2048.0).set("y2", y).set("stroke", "blue").set("stroke-width", 4));
            for x in ipoints {
                doc.append(Circle::new().set("cx",x).set("cy",y).set("r",6).set("fill","blue"));
            }
        }
        svg::save("glyph_intersect.svg", &doc).unwrap();
    }

    #[test]
    fn load_truetype_raster_outline() {
        use truetype_loader::*;
        let mut font_file = File::open(FONT_PATH).unwrap();
        let font = SfntFont::from_binary(&mut font_file).expect("load font data");
        
        let rr = Rasterizer {
            scaler: SimpleGlyphScaler::new(&font, 144.0).expect("create scaler")
        };
        let mut bm = Vec::new();
        bm.resize(512*512, 0u8);

        rr.raster_glyph(test_glyph_index, &mut bm[..], 512, 140.0, Point::new(32.0, 32.0));

        let im = ImageBuffer::from_raw(512,512,bm).unwrap();
        let ref mut fout = File::create(&Path::new("lgloutt.png")).expect("creating output file");
        let _ = image::ImageLuma8(im).save(fout, image::PNG);

    }

    #[test]
    fn load_truetype_raster_string() {
        use truetype_loader::*;
        let mut font_file = File::open(FONT_PATH).unwrap();
        let font = SfntFont::from_binary(&mut font_file).expect("load font data");

        println!("hhea: {:?}", font.hhea_table);

        let rr = Rasterizer {
            scaler: SimpleGlyphScaler::new(&font, 144.0).expect("create scaler")
        };
        let mut bm = Vec::new();
        bm.resize(1024*1024, 0u8);

        let mut point_size = 8.0;

        for i in 0..4 {
            let s = "@Test~String!$&";
            let cm = CharMap::from_truetype(&font);
            let mut offset = Point::new(8.0, 8.0 + (i as f32) * 50.0);
            for c in s.chars() {
                let gi = cm.map(c);
                //let g = Glyph::from_truetype(&font, gi).expect("load glyph");
                rr.raster_glyph(gi, &mut bm[..], 1024, point_size, offset);
                offset.x += font.hmtx_table.as_ref()
                    .map(|hmtx| hmtx.metrics[gi].advance_width as f32 * rr.scaler.uniform_scale(point_size)).unwrap();
            }

            point_size *= 2.0;
        }

        //rr.raster_glyph(&g, &mut bm[..], 512, 24f32);

        let im = ImageBuffer::from_raw(1024,512,bm).unwrap();
        let ref mut fout = File::create(&Path::new("lstrout.png")).expect("creating output file");
        let _ = image::ImageLuma8(im).save(fout, image::PNG);

    }

    #[test]
    fn load_truetype_raster_hinted_string() {
        use truetype_loader::*;
        let mut font_file = File::open(FONT_PATH).unwrap();
        let font = SfntFont::from_binary(&mut font_file).expect("load font data");

        let rr = Rasterizer {
            scaler: interp_instructor::InstructedGlyphScaler::new(&font, 144.0).expect("create scaler")
        };
        let mut bm = Vec::new();
        bm.resize(1024*1024, 0u8);

        let mut point_size = 8.0;

        for i in 0..4 {
            let s = "@Test~String!$&";
            let cm = CharMap::from_truetype(&font);
            let mut offset = Point::new(8.0, 8.0 + (i as f32) * 50.0);
            for c in s.chars() {
                let gi = cm.map(c);
                //let g = Glyph::from_truetype(&font, gi).expect("load glyph");
                rr.raster_glyph(gi, &mut bm[..], 1024, point_size, offset).expect("rasterized glyph");
                offset.x += font.hmtx_table.as_ref()
                    .map(|hmtx| hmtx.metrics[gi].advance_width as f32 * rr.scaler.uniform_scale(point_size)).unwrap();
            }

            point_size *= 2.0;
        }

        //rr.raster_glyph(&g, &mut bm[..], 512, 24f32);

        let im = ImageBuffer::from_raw(1024,512,bm).unwrap();
        let ref mut fout = File::create(&Path::new("hstrout.png")).expect("creating output file");
        let _ = image::ImageLuma8(im).save(fout, image::PNG);

    }
}
