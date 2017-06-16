
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
    let len = (end-start);
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
                let ifwidth = 1f32/((x_max-x_min) as f32);
                let ifheight = 1f32/((y_max-y_min) as f32);
                println!("{} {}", ifwidth, ifheight);
                let mut points = spoints.iter()
                    .map(|&GlyphPoint { x, y, .. }| Point { x: (x as f32)*ifwidth, y: (y as f32)*ifheight }).collect::<Vec<_>>();
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
                                    points.push(Point { x: (midx as f32)*ifwidth, y: (midy as f32)*ifheight });
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
                        let p = (self.points[ista], self.points[ictl], self.points[iend]);
                        let det = -2f32*fy*p.1.y + p.0.y*(fy-p.2.y) + fy*p.2.y + p.1.y*p.1.y;
                        if det < 0f32 { continue; }
                        let A = p.0.y - 2f32*p.1.y + p.2.y;
                        if A == 0f32 { continue; }
                        let ta =  (det.sqrt()-p.0.y+p.1.y)/A;
                        let tb = -(det.sqrt()+p.0.y-p.1.y)/A;
                        if ta > 0f32 && ta < 1f32 { fxs.push((1f32-ta)*(1f32-ta)*p.0.x + 2f32*(1f32-ta)*ta*p.1.x + ta*ta*p.2.x); }
                        if tb > 0f32 && tb < 1f32 { fxs.push((1f32-tb)*(1f32-tb)*p.0.x + 2f32*(1f32-tb)*tb*p.1.x + tb*tb*p.2.x); }
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
            fxs.as_mut_slice().sort_by(|a,b| if a < b { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater });
            println!("{:?}", fxs);

            for sfx in fxs.chunks(2) { //.iter().map(|&v| v.abs()) {
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

    #[test]
    fn raster_glyph_outline() {
        let g = Glyph {
            points: vec![Point{x:0f32,y:0f32},
                            Point{x:0.5f32,y:1f32},
                            Point{x:0.5f32,y:0.5f32},
                            Point{x:0f32,y:1f32},
                            Point{x:1f32,y:1f32}],
            curves: vec![Curve::Quad(0,1,2),Curve::Quad(3,2,4),Curve::Line(0,4)]
        };

        let mut bm = Vec::new();
        bm.resize(512*512, 0u8);
        g.raster_outline(bm.as_mut_slice(), 512, 512);

        let im = ImageBuffer::from_raw(512,512,bm).unwrap();
        let ref mut fout = File::create(&Path::new("gloutt.png")).expect("creating output file");
        let _ = image::ImageLuma8(im).save(fout, image::PNG);
    }

    const test_glyph_index: usize = 9;

    #[test]
    fn load_truetype_svg_out() {
        use truetype_loader::*;
        let mut font_file = File::open("C:\\Windows\\Fonts\\arial.ttf").unwrap();
        let font = SfntFont::from_binary(&mut font_file).expect("load font data");

        let g = Glyph::from_truetype(&font.glyf_table.expect("glyf table").glyphs[test_glyph_index]).unwrap();

        use self::svg::{Document,Node};
        use self::svg::node::element::{Text, Path, Rectangle, Circle, Group};
        use self::svg::node::element::path::Data;

        let scale = 500f32;

        let mut doc = Document::new();
        let mut gr = Group::new();
        for c in g.curves {
            match c {
                Curve::Line(start, end) => {
                    let mut c = Data::new();
                    c = c.move_to((g.points[start].x*scale, g.points[start].y*scale));
                    c = c.line_to((g.points[end].x*scale, g.points[end].y*scale));
                    gr.append(Path::new().set("fill","none").set("stroke","orange").set("stroke-width",6).set("d",c));
                },
                Curve::Quad(start, ctl, end) => {
                    let mut c = Data::new();
                    c = c.move_to((g.points[start].x*scale, g.points[start].y*scale));
                    c = c.quadratic_curve_to((g.points[ctl].x*scale, g.points[ctl].y*scale, g.points[end].x*scale, g.points[end].y*scale));
                    gr.append(Path::new().set("fill","none").set("stroke","orangered").set("stroke-width",6).set("d",c));
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
        doc.assign("viewBox", (0f32, -50f32, scale*2f32, scale*2f32));

        svg::save("glyph_conv.svg", &doc).unwrap();
    }

    #[test]
    fn load_truetype_raster_outline() {
        use truetype_loader::*;
        let mut font_file = File::open("C:\\Windows\\Fonts\\arial.ttf").unwrap();
        let font = SfntFont::from_binary(&mut font_file).expect("load font data");
        
        let g = Glyph::from_truetype(&font.glyf_table.expect("glyf table").glyphs[test_glyph_index]).unwrap();

        let mut bm = Vec::new();
        bm.resize(512*512, 0u8);

        g.raster_outline(bm.as_mut_slice(), 512, 512);

        let im = ImageBuffer::from_raw(512,512,bm).unwrap();
        let ref mut fout = File::create(&Path::new("lgloutt.png")).expect("creating output file");
        let _ = image::ImageLuma8(im).save(fout, image::PNG);

    }
}
