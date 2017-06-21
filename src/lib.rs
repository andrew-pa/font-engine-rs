
#[macro_use]
extern crate bitflags;
extern crate byteorder;
mod truetype_loader;

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

#[derive(Copy,Clone,Debug)]
pub enum Curve {
    Line(usize,usize),
    Quad(usize,usize,usize) // (start, ctrl, end)
}
#[derive(Debug)]
pub struct Glyph {
    curves: Vec<Curve>,
    points: Vec<Point>,
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
    pub fn from_truetype(glyph: &truetype_loader::GlyphDescription) -> Option<Glyph> {
        use truetype_loader::*;
        println!("{:?}", glyph);
        match glyph {
            &GlyphDescription::Simple { 
                num_contours, x_min, x_max, y_min, y_max, ref end_points_of_contours, ref instructions, points: ref spoints 
            } => {
                let mut curves = Vec::new();
                let mut points = spoints.iter()
                    .map(|&GlyphPoint { x, y, .. }| Point { x: x as f32, y: y as f32 }).collect::<Vec<_>>();
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
                            let mut a = spoints[i]; let mut ia = i;
                            i += 1; let mut ib = wrap(i, last_endpoint, endpoint);
                            let mut b = spoints[ib];
                            if b.on_curve {
                                curves.push(Curve::Quad(last_point, ia, ib));
                                last_point = ib;
                            } else {
                                while !b.on_curve {
                                    let midx = (a.x + b.x) / 2;
                                    let midy = (a.y + b.y) / 2;
                                    let im = points.len();
                                    points.push(Point { x: midx as f32, y: midy as f32 });
                                    curves.push(Curve::Quad(last_point, ia, im)); last_point = im;
                                    a = b; ia = ib;
                                    i += 1; ib = wrap(i,last_endpoint,endpoint);
                                    b = spoints[ib];
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

    // render to grayscale, no AA
    /*
    pub fn raster_outline<'a>(&self, bitmap: &'a mut [u8], width: usize, height: usize) -> &'a [u8] {
        
        let ifheight = 1f32 / (height as f32);
        let fwidth  = width as f32;

        for y in 0..height {
            // calculate intersection points
            let fy = (y as f32) * ifheight;

            let mut fxs : Vec<f32> = Vec::new();
            for c in &self.curves {
                match c {
                    &Curve::Quad(ista, ictl, iend) => {
                        let mut p = (self.points[ista], self.points[ictl], self.points[iend]);
                        if p.0.x > p.2.x { let tmp = p.0; p.0 = p.2; p.2 = tmp; }
                        let det = -2f32*fy*p.1.y + p.0.y*(fy-p.2.y) + fy*p.2.y + p.1.y*p.1.y;
                        if det < 0f32 { continue; }
                        let A = p.0.y - 2f32*p.1.y + p.2.y;
                        if A == 0f32 { continue; }
                        let ta =  (det.sqrt()-p.0.y+p.1.y)/A;
                        let tb = -(det.sqrt()+p.0.y-p.1.y)/A;
                        //if ta > 0f32 && ta < 1f32 
                        { fxs.push((1f32-ta)*(1f32-ta)*p.0.x + 2f32*(1f32-ta)*ta*p.1.x + ta*ta*p.2.x); }
                        //if tb > 0f32 && tb < 1f32 
                        { fxs.push((1f32-tb)*(1f32-tb)*p.0.x + 2f32*(1f32-tb)*tb*p.1.x + tb*tb*p.2.x); }
                    },
                    &Curve::Line(ista, iend) => {
                        let mut p = (self.points[ista], self.points[iend]);
                        if p.0.y > p.1.y { let tmp = p.0; p.0 = p.1; p.1 = tmp; }
                        if fy < p.0.y || fy > p.1.y { continue; }
                        if p.1.x == p.0.x {
                            print!("v");
                            fxs.push(p.0.x);
                        } else if p.1.y == p.0.y {
                            print!("h");
                            fxs.push(p.0.x);
                            fxs.push(p.1.x);
                        } else {
                            let m = (p.1.y-p.0.y)/(p.1.x-p.0.x);
                            // y-y1 = m(x-x1)
                            fxs.push( (fy-p.0.y)/m + p.0.x );
                        }
                    }
                }
            }
            //fxs.as_mut_slice().sort_by(|a,b| if a < b { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater });
            println!("{:?}", fxs);

            /*for sfx in fxs.chunks(2) { //.iter().map(|&v| v.abs()) {
                if sfx.len() == 1 {
                    let fx = sfx[0];
                    if fx < 0f32 || fx > 1f32 { continue; }
                    let x = (fx * fwidth) as usize;
                    bitmap[y * width + x] = 255;
                } else if sfx.len() == 2 {
                    let x0 = (sfx[0]*fwidth) as usize;
                    let x1 = (sfx[1]*fwidth) as usize;
                    for x in x0..x1 {
                        if x < 0 || x > width { continue; }
                        bitmap[y * width + x] = 255;
                    }
                } else { panic!("???"); }
            }*/
            for fx in fxs.iter().map(|&v| v.abs()) {
                if fx < 0f32 || fx > 1f32 { continue; }
                let x = (fx*fwidth) as usize;
                bitmap[y * width + x] = 255;
            }
        }
        bitmap
    } */
}

pub struct Rasterizer {
    output_dpi: f32,
    units_per_em: f32
}

impl Curve {
    
    // intersects this curve with a test ray that goes along the +Y direction from the point (x,y)
    fn intersects_test_ray(&self, points: &Vec<Point>, tx: f32, y: f32) -> bool {
        match self {
            &Curve::Line(start, end) => {
                // y-y1 = m(x-x1)
                // y = $y; is there an x value that satisfies? x = (y-y1)/m + x1 
                // x must be less than end.x and greater than start.x
                let m = (points[end].y - points[start].y) / (points[end].x - points[start].x);
                let x = (y - points[start].y)/m + points[start].x;
                x < points[end].x && x >= tx
            },
            &Curve::Quad(start, ctrl, end) => {
                // (x,y) = (1-t)²p₀ + 2*(1-t)*t*p₁ + t²p₂
                // y = $y; there are two t values that satisfy, and the x values can be found using
                // the original equation. If we only wish to check existance, only the determinant
                // matters
                let a = points[start].y; let b = points[ctrl].y; let c = points[end].y;
                let det = -a*c + a*y + b*b - 2f32*b*y + c*y + a-b;
                det > 0f32
            }
        }
    }
}

impl Rasterizer {

    pub fn raster_glyph<'a>(&self, glyph: &Glyph, bitmap: &'a mut [u8], width: usize, point_size: f32) -> &'a [u8] {
        let scale = point_size * self.output_dpi / (72f32 * self.units_per_em);
        let points: Vec<Point> = glyph.points.iter().map(|&p| Point { x: p.x * scale, y: p.y * scale }).collect();
        //grid fit the outline
        // this involves interpreting some instructions
        //rasterize by scan line
        let height = bitmap.len() / width;
        for y in 0..height {
            for x in 0..width {
                let mut count = 0;
                for c in &glyph.curves {
                    if c.intersects_test_ray(&points, x as f32, y as f32) {
                        let (start,end) = match c {
                            &Curve::Line(start,end) => (start,end),
                            &Curve::Quad(start,_,end) => (start,end)
                        };
                        if points[start].y < points[end].y { //|| points[start].x > points[end].x {
                            // contour crossed from right/left or bottom/top
                            count += 1;
                        } else {
                            // contour crossed from left/right or top/bottom
                            count -= 1;
                        }
                    }
                }
                if count > 0 {
                    bitmap[x + y*width] = 255;
                }
            }
        }
        bitmap
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
    use self::svg::node::element::{Text, Path as GPath, Rectangle, Circle, Group};
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

    const test_glyph_index: usize = 6;

    #[test]
    fn load_truetype_svg_out() {
        use truetype_loader::*;
        let mut font_file = File::open("C:\\Windows\\Fonts\\arial.ttf").unwrap();
        let font = SfntFont::from_binary(&mut font_file).expect("load font data");

        let g = Glyph::from_truetype(&font.glyf_table.expect("glyf table").glyphs[test_glyph_index]).unwrap();
        let doc = glyph_to_svg(&g, 0.5f32);
        svg::save("glyph_conv.svg", &doc).unwrap();
    }

    #[test]
    fn load_truetype_raster_outline() {
        use truetype_loader::*;
        let mut font_file = File::open("C:\\Windows\\Fonts\\arial.ttf").unwrap();
        let font = SfntFont::from_binary(&mut font_file).expect("load font data");
        
        let g = Glyph::from_truetype(&font.glyf_table.expect("glyf table").glyphs[test_glyph_index]).unwrap();

        let rr = Rasterizer { output_dpi: 144f32, units_per_em: font.head_table.expect("head table").units_per_em as f32 };
        println!("u/em = {}", rr.units_per_em);
        let mut bm = Vec::new();
        bm.resize(512*512, 0u8);

        rr.raster_glyph(&g, bm.as_mut_slice(), 512, 200f32);

        let im = ImageBuffer::from_raw(512,512,bm).unwrap();
        let ref mut fout = File::create(&Path::new("lgloutt.png")).expect("creating output file");
        let _ = image::ImageLuma8(im).save(fout, image::PNG);

    }
}
