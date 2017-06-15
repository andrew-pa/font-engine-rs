
#[macro_use]
extern crate bitflags;
extern crate byteorder;
mod truetype_loader;

use std::error::Error;

#[derive(Copy,Clone,Debug)]
pub struct Point {
    x: f32, y: f32
}

pub struct Glyph {
    curves: Vec<(usize,usize,usize)>, // (start, ctrl, end)
    points: Vec<Point>,
}

impl Glyph {

    // render to grayscale, no AA
    pub fn raster_outline(&self, bitmap: &mut [u8], width: usize, height: usize) -> &[u8] {
        
        let ifheight = 1f32 / (height as f32);
        let fwidth  = width as f32;

        for y in 0..height {
            // calculate intersection points
            let fy = (y as f32) * ifheight;

            let mut fxs : Vec<f32> = Vec::new();
            for p in self.curves.iter()
                        .map(|&(sta,ctl,end)| (self.points[sta], self.points[ctl], self.points[end])) {
                let det = -2f32*fy*p.1.y + p.0.y*(fy-p.2.y) + fy*p.2.y + p.1.y*p.1.y;
                if det < 0f32 { continue; }
                let A = p.0.y - 2f32*p.1.y + p.2.y;
                if A == 0f32 { continue; }
                let ta =  (det.sqrt()-p.0.y+p.1.y)/A;
                let tb = -(det.sqrt()+p.0.y-p.1.y)/A;
                fxs.push((1f32-ta)*(1f32-ta)*p.0.x + 2f32*(1f32-ta)*ta*p.1.x + ta*ta*p.2.x); 
                fxs.push((1f32-tb)*(1f32-tb)*p.0.x + 2f32*(1f32-tb)*tb*p.1.x + tb*tb*p.2.x);
            }
            println!("{:?}", fxs);

            for fx in fxs.iter().map(|&v| v.abs()) {
                if fx < 0f32 || fx > 1f32 { continue; }
                println!("{}", fx);
                let x = (fx * fwidth) as usize;
                bitmap[y * width + x] = 255;
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

    #[cfg(test)]
    extern crate image;
    use self::image::{ImageBuffer,Luma,Pixel};

    #[test]
    fn raster_glyph_outline() {
        let g = Glyph {
            points: vec![Point{x:0f32,y:0f32},
                            Point{x:0.5f32,y:1f32},
                            Point{x:0.5f32,y:0.5f32},
                            Point{x:0f32,y:1f32},
                            Point{x:1f32,y:1f32}],
            curves: vec![(0,1,2),(3,2,4)]
        };

        let mut bm = [0u8; 512*512];
        g.raster_outline(&mut bm, 512, 512);

        bm[0] = 255;

        let im = ImageBuffer::from_raw(512,512,&bm).unwrap();
        let ref mut fout = File::create(&Path::new("gloutt.png")).expect("creating output file");
        let _ = image::ImageLuma8(im).save(fout, image::PNG);
    }
}
