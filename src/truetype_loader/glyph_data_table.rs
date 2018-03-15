use std::fmt;
use std::fmt::{Debug};
use std::mem;
use std::fs::File;
use byteorder::{ByteOrder, BigEndian, ReadBytesExt};

use super::*;

use numerics::F2dot14;

#[derive(Debug)]
pub enum Transformation {
    Uniform(F2dot14),
    XY(F2dot14, F2dot14),
    Mat2x2 {
        xscale: F2dot14,
        scale01: F2dot14,
        scale10: F2dot14,
        yscale: F2dot14
    }
}

bitflags! {
    flags ComponentGlyphFlags: u16 {
        const CGF_ARGS_ARE_WORDS        = 0b0000_0000_0000_0001,
        const CGF_ARGS_ARE_XY           = 0b0000_0000_0000_0010,
        const CGF_ROUND_XY_TO_GRID      = 0b0000_0000_0000_0100,
        const CGF_SIMPLE_SCALE          = 0b0000_0000_0000_1000,
        const CGF_MORE_COMPONENTS       = 0b0000_0000_0010_0000,
        const CGF_XY_SCALE              = 0b0000_0000_0100_0000,
        const CGF_2X2_TRANSFORM         = 0b0000_0000_1000_0000,
        const CGF_INSTRUCTIONS_PRESENT  = 0b0000_0001_0000_0000,
        const CGF_USE_METRICS           = 0b0000_0010_0000_0000,
        const CGF_OVERLAP_COMPOUND      = 0b0000_0100_0000_0000
    }
}

#[derive(Debug)]
pub struct ComponentGlyphDescription {
    glyph_index: u16,
    arg1: u16,
    arg2: u16,
    transform: Transformation,
    use_metrics: bool
}


bitflags! {
    flags GlyphPointFlags: u8 {
        const GP_RESERVED       = 0b1100_0000,
        const GP_OnCurve        = 0b0000_0001,
        const GP_XShortVec      = 0b0000_0010,
        const GP_YShortVec      = 0b0000_0100,
        const GP_Repeat         = 0b0000_1000,
        const GP_XSameOrVecSign = 0b0001_0000,
        const GP_YSameOrVecSign = 0b0010_0000,
    }
}

#[derive(Copy, Clone, Debug)]
pub struct GlyphPoint {
    pub on_curve: bool,
    pub x: i32, pub y: i32,
    flag: GlyphPointFlags
}

pub enum GlyphDescription {
    None,
    Simple {
        num_contours: u16,
        x_min: i16,
        y_min: i16,
        x_max: i16,
        y_max: i16,
        end_points_of_contours: Vec<u16>,
        instructions: Vec<u8>,
        points: Vec<GlyphPoint>,
    },
    Composite {
        components: Vec<ComponentGlyphDescription>,
        instructions: Vec<u8>
    }
}

impl Debug for GlyphDescription {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &GlyphDescription::None => write!(f, "GlyphDescription::None"),
            &GlyphDescription::Simple { 
                num_contours: _, x_min: _, x_max: _, y_min: _, y_max: _,
                end_points_of_contours: ref epoc,
                instructions: ref is,
                points: ref p
            } => write!(f, "GlyphDescription::Simple [ epocs={:?}, instrs={}, points={} ]", epoc, is.len(), p.len()),
            &GlyphDescription::Composite{ components: ref g, instructions: _ } => write!(f, "GlyphDescription::Compound {:?}", g)
        }
    }
}

impl GlyphDescription {
    fn from_binary<R: Read+Seek>(reader: &mut R, num_points: usize, glyph_length: usize) -> io::Result<GlyphDescription> {
        //println!("reading glyph p{} l{}", num_points, glyph_length);
        if glyph_length == 0 { println!("0-len glyph?"); return Ok(GlyphDescription::None); }
        let num_contours = reader.read_i16::<BigEndian>()?;
        //println!("num contours = {}", num_contours);
        let x_min = reader.read_i16::<BigEndian>()?;
        let y_min = reader.read_i16::<BigEndian>()?;
        let x_max = reader.read_i16::<BigEndian>()?;
        let y_max = reader.read_i16::<BigEndian>()?;
        if glyph_length == 0 { return Ok(GlyphDescription::None); }
        if num_contours > 0 {
            let mut epoc = Vec::new();
            for _ in 0..num_contours {
                epoc.push(reader.read_u16::<BigEndian>()?);
            }
            //println!("end points of contours = {:?}", epoc);
            let num_instr = reader.read_u16::<BigEndian>()?;
            let mut instr = vec![0u8; num_instr as usize];
            reader.read_exact(instr.as_mut_slice())?;
            let mut data = vec![0u8; glyph_length];//-(10+epoc.len()*2+instr.len())]; // TODO: the loader seems to consistantly read more than what would be expected from this buffer, so it reads more than should be necessary to allow that. Questionable indeed.
            reader.read_exact(data.as_mut_slice())?;
            let mut points = Vec::new();
            let mut i: usize = 0;
            let n = (epoc[epoc.len()-1]+1) as usize; //this seems to be a guess found in STB's truetype loader
            while i < data.len() {
                let d0 = data[i];
                let flag = GlyphPointFlags::from_bits_truncate(d0);
                //println!("{} point [ flags = {:b}/{:?} ]", points.len(), d0, flag);
                i += 1;
                let repeat_count = 
                    if flag.intersects(GP_Repeat) {
                        let v = data[i];
                        //println!("repeat = {}", v);
                        i += 1;
                        v + 1
                    } else {
                        1
                    };
                for _ in 0..repeat_count {
                    points.push(GlyphPoint{on_curve: flag.intersects(GP_OnCurve), x: 0, y: 0, flag: flag });
                    if points.len() >= n { break; }
                }
                if points.len() >= n { break; }
            }
            //println!("found {} points of {}, ifl = {}, d.l = {}, left={}", points.len(), n, i, data.len(), data.len()-i);
            assert!(points.len() < data.len(), "absurd number of points!");

            fn load_vec(data: &Vec<u8>, i: &mut usize, last: &mut i32, short_vec: bool, sameorsign: bool) -> i32 {
                if short_vec {
                    let v = (data[*i] as i32) * if sameorsign {1} else {-1};
                    *last += v;
                    *i += 1;
                } else if !sameorsign {
                    let v = (data[*i] as u16)*256 + data[(*i) + 1] as u16;
                    *last = last.wrapping_add((v as i16) as i32);
                    assert!(last.abs() < 30000);
                    *i += 2;
                    //print!("2");
                } //else { print!("!!! "); }
                //println!("i{} v{}", *i, *last); 
                *last
            }

            let mut last: i32 = 0;
            for mut p in &mut points {
                //if p.flag.intersects(GP_Repeat) { /*print!("REP ");*/ }
                p.x = load_vec(&data, &mut i, &mut last, p.flag.intersects(GP_XShortVec), p.flag.intersects(GP_XSameOrVecSign));
            }
            //println!("---");
            last = 0;
            let mut count = 0;
            for mut p in &mut points {
                //if p.flag.intersects(GP_Repeat) { print!("REP "); }
                p.y = load_vec(&data, &mut i, &mut last, p.flag.intersects(GP_YShortVec), p.flag.intersects(GP_YSameOrVecSign));
                count+=1;
                //print!("c{} ", count);
            }
            Ok(GlyphDescription::Simple {
                num_contours: num_contours as u16,
                x_min: x_min,
                y_min: y_min,
                x_max: x_max,
                y_max: y_max,
                end_points_of_contours: epoc,
                instructions: instr,
                points: points
            })
        } else if num_contours < 0 {
            let mut components = Vec::new();
            let mut has_instructions = false;
            loop {
                let flags = ComponentGlyphFlags::from_bits_truncate(reader.read_u16::<BigEndian>()?);
                //println!("flags = {:?}", flags);
                let ix = reader.read_u16::<BigEndian>()?;
                let (arg1, arg2) =
                    if flags.intersects(CGF_ARGS_ARE_WORDS) {
                        (reader.read_u16::<BigEndian>()?, reader.read_u16::<BigEndian>()?)
                    } else {
                        let arg12 = reader.read_u8()?;
                        (arg12 as u16 >> 8, arg12 as u16 & 0x00ff)
                    };
                let tf = if flags.intersects(CGF_SIMPLE_SCALE) {
                    Transformation::Uniform(F2dot14::new(reader.read_i16::<BigEndian>()?))
                } else if flags.intersects(CGF_XY_SCALE) {
                    Transformation::XY(F2dot14::new(reader.read_i16::<BigEndian>()?), F2dot14::new(reader.read_i16::<BigEndian>()?))
                } else if flags.intersects(CGF_2X2_TRANSFORM) {
                    Transformation::Mat2x2 {
                        xscale: F2dot14::new(reader.read_i16::<BigEndian>()?), 
                        scale01:F2dot14::new(reader.read_i16::<BigEndian>()?), 
                        scale10:F2dot14::new(reader.read_i16::<BigEndian>()?), 
                        yscale: F2dot14::new(reader.read_i16::<BigEndian>()?) 
                    }
                } else {
                    Transformation::Uniform(F2dot14::new(0b0100_0000_0000_0000))
                }; 

                //TODO: Apple's manual has some math that seems to generate a matrix. Should that
                //go here? Transforms are definitly not finsihed out it seem
                
                components.push(ComponentGlyphDescription {
                    glyph_index: ix,
                    arg1: arg1, arg2: arg2,
                    transform: tf,
                    use_metrics: flags.intersects(CGF_USE_METRICS)
                });

                if flags.intersects(CGF_INSTRUCTIONS_PRESENT) {
                    has_instructions = true;
                }
                if !flags.intersects(CGF_MORE_COMPONENTS) {
                    break;
                }
            }
            let instr = if has_instructions {
                let num_instr = reader.read_u16::<BigEndian>()?;
                //println!("reading {} instrs", num_instr);
                let mut i = vec![0u8; num_instr as usize];
                reader.read_exact(i.as_mut_slice())?;
                i
            } else { /*println!("no instrs");*/ vec![0u8,0] };
            Ok(GlyphDescription::Composite{components:components,instructions:instr})
        } else { //this might be invalid, you might be supposed to read a single glyph anyway, but i fail to see how there
                 //could be glyph data if there are no contours
            /*println!("no glyph data?");*/     
            Ok(GlyphDescription::None)
        }
    }
}

// apparently this table is useless
#[derive(Debug)]
pub struct GlyphDataTable {
    pub glyphs: Vec<GlyphDescription>

}

impl Table for GlyphDataTable {
    fn tag(&self) -> TableTag { TableTag::GlyphData }
}


impl GlyphDataTable {
    /// This function reads a 'glyf' table from a file, assymbling the glyphs togther as it goes
    /// using data from the 'loca' table
    pub fn from_binary<R: Read+Seek>(reader: &mut R, table_start: u64, maxp_table: MaxProfileTable, loca_table: &LocationTable) -> io::Result<GlyphDataTable> {
        let mut glyphs = Vec::new();
        for glyph_ix in loca_table.offsets.windows(2) {
            //println!("glyph_ix = {:?}", glyph_ix);
            reader.seek(io::SeekFrom::Start(table_start + glyph_ix[0] as u64))?;
            glyphs.push(GlyphDescription::from_binary(reader, maxp_table.num_points as usize, (glyph_ix[1]-glyph_ix[0]) as usize)?);
        }
        Ok(GlyphDataTable {glyphs: glyphs})
    }
}
