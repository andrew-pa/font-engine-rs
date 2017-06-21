#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(non_upper_case_globals)]
use std::io;
use std::io::prelude::*;
use std::fmt;
use std::fmt::{Debug};
use std::mem;
use std::rc::Rc;
use byteorder::{ByteOrder, BigEndian, ReadBytesExt};


/* from Microsoft's TrueType spec:
Data Types
The following data types are used in the TrueType font file. All TrueType fonts use Motorola-style byte ordering (Big Endian):

Data type	Description
BYTE	8-bit unsigned integer.
CHAR	8-bit signed integer.
USHORT	16-bit unsigned integer.
SHORT	16-bit signed integer.
ULONG	32-bit unsigned integer.
LONG	32-bit signed integer.
FIXED	32-bit signed fixed-point number (16.16)
FUNIT	Smallest measurable distance in the em space.
FWORD	16-bit signed integer (SHORT) that describes a quantity in FUnits.
UFWORD	Unsigned 16-bit integer (USHORT) that describes a quantity in FUnits.
F2DOT14	16-bit signed fixed number with the low 14 bits of fraction (2.14).
*/

//TODO: Change this so that it just converts to float?
#[derive(Copy, Clone, Debug)]
pub struct Fixed {
    int_part: u16,
    frac_part: u16
}
impl Fixed {
    pub fn from_binary<R: Read + Seek, E: ByteOrder>(r: &mut R) -> io::Result<Fixed> {
        Ok(Fixed{ int_part: r.read_u16::<E>()?, frac_part: r.read_u16::<E>()? })
    }
}

//TODO: Change this so that it just converts to float, silly fixed point is silly
#[derive(Copy, Clone, Debug)]
pub struct F2dot14(i16);

macro_rules! table_tag_code {
    ($a:expr, $b:expr, $c:expr, $d:expr) => (($a as u32) << 24 | ($b as u32) << 16 | ($c as u32) << 8 | ($d as u32));
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum TableTag {
    //Required Tables
    CharGlyphMapping = table_tag_code!('c','m','a','p'),
    GlyphData = table_tag_code!('g','l','y','f'),
    FontHeader = table_tag_code!('h', 'e', 'a', 'd'),
    HorizHeader = table_tag_code!('h', 'h', 'e', 'a'),
    HorizMetics = table_tag_code!('h', 'm', 't', 'x'),
    LocationIndex = table_tag_code!('l', 'o', 'c', 'a'),
    MaxProfile = table_tag_code!('m', 'a', 'x', 'p'),
    Nameing = table_tag_code!('n', 'a', 'm', 'e'),
    PostScriptInfo = table_tag_code!('p', 'o', 's', 't'),
    WindowsOS2SpecificMetric = table_tag_code!('O', 'S', '/', '2'),
    //Optional Tables
    ControlValue = table_tag_code!('c', 'v', 't', ' '),
    EmbeddedBitmapData = table_tag_code!('E', 'B', 'D', 'T'),
    EmbeddedBitmapLocationData = table_tag_code!('E', 'B', 'L', 'C'),
    EmbeddedBitmapScalingData = table_tag_code!('E', 'B', 'S', 'C'),
    FontProgram = table_tag_code!('f', 'p', 'g', 'm'),
    GridFitAndScanConvertProc = table_tag_code!('g', 'a', 's', 'p'),
    HorizDevMetric = table_tag_code!('h', 'd', 'm', 'x'),
    Kerning = table_tag_code!('k', 'e', 'r', 'n'),
    LinearThreshold = table_tag_code!('L', 'T', 'S', 'H'),
    CVTProgram = table_tag_code!('p', 'r', 'e', 'p'),
    PCL5 = table_tag_code!('P', 'C', 'L', 'T'),
    VertDevMetrics = table_tag_code!('V', 'D', 'M', 'X'),
    VertHeader = table_tag_code!('v', 'h', 'e', 'a'),
    VertMetrics = table_tag_code!('v', 'm', 't', 'x')
}

impl Debug for TableTag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let n = *self as u32;
        write!(f, "Table:{}{}{}{}", ((n>>24) as u8) as char, ((n>>16) as u8) as char, ((n>>8) as u8) as char, (n as u8) as char)
    }
}

pub trait Table : Debug {
    fn tag(&self) -> TableTag;
}

mod char_glyph_mapping_table;
pub use self::char_glyph_mapping_table::*;
mod glyph_data_table;
pub use self::glyph_data_table::*;

pub struct ControlValueTable(Vec<i16>);

impl Table for ControlValueTable {
    fn tag(&self) -> TableTag { TableTag::ControlValue }
}

impl Debug for ControlValueTable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ControlValueTable [len = {}]", self.0.len())
    }
}

pub struct FontProgram(Vec<u8>);

impl Table for FontProgram {
    fn tag(&self) -> TableTag { TableTag::FontProgram }
}
impl Debug for FontProgram {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FontProgram [len = {}]", self.0.len())
    }
}

bitflags! {
    flags GASPBehavior: u16 {
        const GASP_NEITHER = 0x0000u16,
        const GASP_GRIDFIT = 0x1000u16,    //these are tricky because it's in big endian format
        const GASP_GRAYSCALE = 0x2000u16,
    }
}

#[derive(Copy, Clone, Debug)]
pub struct GASPRange {
    range_max_ppem: u16,
    range_gasp_behavior: u16
}

#[derive(Debug)]
pub struct GASPTable {
    version: u16,
    gasp_ranges: Vec<GASPRange>
}

impl Table for GASPTable {
    fn tag(&self) -> TableTag { TableTag::GridFitAndScanConvertProc }
}

impl GASPTable {
    fn from_binary<R: Read + Seek>(reader: &mut R) -> io::Result<GASPTable> {
        let ver = reader.read_u16::<BigEndian>()?;
        let num_ranges = reader.read_u16::<BigEndian>()?;
        let mut r = Vec::new();
        for _ in 0..num_ranges {
            let gb = reader.read_u16::<BigEndian>()?;
            //println!("GASP bits {:b}b ; {:b}b", gb, GASP_GRIDFIT.bits());
            r.push(GASPRange {
                range_max_ppem: reader.read_u16::<BigEndian>()?,
                range_gasp_behavior: /*match GASPBehavior::from_bits(gb) {
                    Some(v) => v,
                    None => return Err(io::Error::new(io::ErrorKind::Other, "Unknown GASP behavior bits"))
                }*/ gb
            });
        }
        return Ok(GASPTable { version: ver, gasp_ranges: r });
    }
}

enum DeviceRecord {
    Format0 {
        pixel_size: u8,
        max_width: u8,
        widths: Vec<u8>
    }
}
impl Debug for DeviceRecord {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &DeviceRecord::Format0 { pixel_size: ps, max_width: mw, widths: ref w } =>
                write!(f, "Format0 {{ pixel_size: {}, max_width: {}, widths: [len = {}] }}", ps, mw, w.len()),
        }
    }
}


#[derive(Debug)]
pub struct HorizDeviceMetricsTable {
    version: u16,
    records: Vec<DeviceRecord>
}

impl HorizDeviceMetricsTable {
    fn from_binary<R: Read+Seek>(reader: &mut R, num_glyphs: usize) -> io::Result<HorizDeviceMetricsTable> {
        let v = reader.read_u16::<BigEndian>()?;
        let num_dr = reader.read_i16::<BigEndian>()?;
        let size_dr = reader.read_i32::<BigEndian>()?;
        let mut r = Vec::new();
        for _ in 0..num_dr {
            let ps = reader.read_u8()?;
            let mw = reader.read_u8()?;
            let mut w = vec![0u8; num_glyphs];
            reader.read_exact(w.as_mut_slice())?;
            r.push(DeviceRecord::Format0 {
                pixel_size: ps,
                max_width: mw,
                widths: w
            }); // this requires knowing numGlyphs from the maxp table
            reader.seek(io::SeekFrom::Current(size_dr as i64))?;
        }
        Ok(HorizDeviceMetricsTable {
            version: v,
            records: r
        })
    }
}

impl Table for HorizDeviceMetricsTable {
    fn tag(&self) -> TableTag { TableTag::HorizDevMetric }
}

#[derive(Copy, Clone, Debug)]
pub struct FontHeader {
    pub version: Fixed,
    pub font_rev: Fixed,
    pub checksum: u32,
    pub flags: u16,
    pub units_per_em: u16,
    pub created: u64,
    pub modified: u64,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
    pub mac_style: u16,
    pub lowest_rec_ppem: u16,
    pub font_direction_hint: i16,
    pub index_to_locformat: i16,
    pub glyph_data_format: i16
}

impl FontHeader {
    fn from_binary<R: Read + Seek>(reader: &mut R) -> io::Result<FontHeader> {
        Ok(FontHeader {
            version: Fixed::from_binary::<R,BigEndian>(reader)?,
            font_rev: Fixed::from_binary::<R,BigEndian>(reader)?,
            checksum: reader.read_u32::<BigEndian>()?,
            flags: { assert_eq!(reader.read_u32::<BigEndian>()?, 0x5f0f3cf5, "invalid magic"); reader.read_u16::<BigEndian>()? },
            units_per_em: reader.read_u16::<BigEndian>()?,
            created: reader.read_u64::<BigEndian>()?,
            modified: reader.read_u64::<BigEndian>()?,
            x_min: reader.read_i16::<BigEndian>()?,
            y_min: reader.read_i16::<BigEndian>()?,
            x_max: reader.read_i16::<BigEndian>()?,
            y_max: reader.read_i16::<BigEndian>()?,
            mac_style: reader.read_u16::<BigEndian>()?,
            lowest_rec_ppem: reader.read_u16::<BigEndian>()?,
            font_direction_hint: reader.read_i16::<BigEndian>()?,
            index_to_locformat: reader.read_i16::<BigEndian>()?,
            glyph_data_format: reader.read_i16::<BigEndian>()?,
        })
    }
}

impl Table for FontHeader {
    fn tag(&self) -> TableTag { TableTag::FontHeader }
}

#[derive(Copy,Clone,Debug)]
pub struct MaxProfileTable {
    version: Fixed,
    num_glyphs: u16,
    num_points: u16,
    max_contours: u16,
    max_composite_points: u16,
    max_composite_contours: u16,
    max_zones: u16,
    max_twilight_points: u16,
    max_storage: u16,
    max_function_defs: u16,
    max_instruction_defs: u16,
    max_stack: u16,
    max_instruction_size: u16,
    max_component_elements: u16,
    max_component_depth: u16
}

impl Table for MaxProfileTable {
    fn tag(&self) -> TableTag { TableTag::FontHeader }
}

impl MaxProfileTable {
    fn from_binary<R: Read + Seek>(reader: &mut R) -> io::Result<MaxProfileTable> {
        Ok(MaxProfileTable {
            version: Fixed::from_binary::<R,BigEndian>(reader)?,
            num_glyphs: reader.read_u16::<BigEndian>()?,
            num_points: reader.read_u16::<BigEndian>()?,
            max_contours: reader.read_u16::<BigEndian>()?,
            max_composite_points: reader.read_u16::<BigEndian>()?,
            max_composite_contours: reader.read_u16::<BigEndian>()?,
            max_zones: reader.read_u16::<BigEndian>()?,
            max_twilight_points: reader.read_u16::<BigEndian>()?,
            max_storage: reader.read_u16::<BigEndian>()?,
            max_function_defs: reader.read_u16::<BigEndian>()?,
            max_instruction_defs: reader.read_u16::<BigEndian>()?,
            max_stack: reader.read_u16::<BigEndian>()?,
            max_instruction_size: reader.read_u16::<BigEndian>()?,
            max_component_elements: reader.read_u16::<BigEndian>()?,
            max_component_depth: reader.read_u16::<BigEndian>()?
        })
    }
}

#[derive(Clone)]
pub struct LocationTable {
    offsets: Vec<u32>
}

impl Debug for LocationTable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "LocationTable len={}", self.offsets.len())
    }
}

impl Table for LocationTable {
    fn tag(&self) -> TableTag { TableTag::LocationIndex }
}

impl LocationTable {
    fn from_binary<R: Read + Seek>(reader: &mut R, num_glyphs: usize, format: i16) -> io::Result<LocationTable> {
        Ok(LocationTable {
            offsets: {
                let mut v = Vec::new();
                for _ in 0..(num_glyphs+1) {
                    v.push(if format == 1 { reader.read_u32::<BigEndian>()? } else { reader.read_u16::<BigEndian>()? as u32 *2 })
                }
                v
            }
        })
    }
}


#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct TableDirectoryEntry {
    tag: TableTag, check_sum: u32, offset: u32, length: u32
}
impl TableDirectoryEntry {
    fn from_binary<R: Read + Seek>(reader: &mut R) -> io::Result<TableDirectoryEntry> {
        Ok(TableDirectoryEntry {
            tag: unsafe { mem::transmute(reader.read_u32::<BigEndian>()?) },
            check_sum: reader.read_u32::<BigEndian>()?,
            offset: reader.read_u32::<BigEndian>()?,
            length: reader.read_u32::<BigEndian>()?
        })
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct SfntFont {
    pub sfnt_version: Fixed,
    pub search_range: u16,
    pub entry_selector: u16,
    pub range_shift: u16,
    pub table_directory: Vec<TableDirectoryEntry>,
    pub cmap_table: Option<CharGlyphMappingTable>,
    pub cval_table: Option<ControlValueTable>,
    pub fprg_table: Option<FontProgram>,
    pub gasp_table: Option<GASPTable>,
    pub glyf_table: Option<GlyphDataTable>,
    pub loca_table: Option<LocationTable>,
    pub hdmx_table: Option<HorizDeviceMetricsTable>,
    pub head_table: Option<FontHeader>,
    pub maxp_table: Option<MaxProfileTable>
}

impl SfntFont {
    pub fn from_binary<R: Read + Seek>(reader : &mut R) -> io::Result<SfntFont> {
        let version = Fixed::from_binary::<R,BigEndian>(reader)?;
        let num_tables = reader.read_u16::<BigEndian>()?;
        let search_range = reader.read_u16::<BigEndian>()?;
        let entry_sel = reader.read_u16::<BigEndian>()?;
        let range_shift = reader.read_u16::<BigEndian>()?;
        let mut table_directory = Vec::new();
        for _ in 0..num_tables {
            let tbe = TableDirectoryEntry::from_binary(reader)?;
            match tbe.tag {
                TableTag::MaxProfile => table_directory.insert(0, tbe),
                TableTag::FontHeader => table_directory.insert(1, tbe),
                TableTag::LocationIndex => table_directory.insert(2, tbe),
                _ => table_directory.push(tbe)
            }
        }
        //println!("table directory: {:?}", table_directory);
        let mut fnt = SfntFont {
            sfnt_version: version,
            search_range: search_range,
            entry_selector: entry_sel,
            range_shift: range_shift,
            table_directory: table_directory,
            cmap_table: None,
            cval_table: None,
            fprg_table: None,
            gasp_table: None,
            glyf_table: None,
            loca_table: None,
            hdmx_table: None,
            head_table: None,
            maxp_table: None,
        };
        for tde in &fnt.table_directory {
            reader.seek(io::SeekFrom::Start(tde.offset as u64))?;
            match tde.tag {
                TableTag::CharGlyphMapping =>
                   fnt.cmap_table = Some(char_glyph_mapping_table::CharGlyphMappingTable::from_binary(reader, tde.offset as u64)?),
                TableTag::ControlValue => {
                    let mut tbl = Vec::with_capacity((tde.length/2) as usize);
                    for _ in 0..tde.length {
                        tbl.push(reader.read_i16::<BigEndian>()?);
                    }
                    fnt.cval_table = Some(ControlValueTable(tbl))
                },
                TableTag::FontProgram => {
                    let mut tbl = vec![0u8; tde.length as usize];
                    reader.read_exact(tbl.as_mut_slice())?;
                    fnt.fprg_table = Some(FontProgram(tbl))
                },
                TableTag::GridFitAndScanConvertProc =>
                    fnt.gasp_table = Some(GASPTable::from_binary(reader)?),
                TableTag::GlyphData =>
                    fnt.glyf_table = Some(GlyphDataTable::from_binary(reader, tde.offset as u64,
                                    fnt.maxp_table.ok_or(io::Error::new(io::ErrorKind::Other, "Must load maxp table before glyf table!"))?,
                                    fnt.loca_table.as_ref().ok_or(io::Error::new(io::ErrorKind::Other, "Must load loca table before glyf table!"))? )?),
                TableTag::LocationIndex => {
                    fnt.loca_table = Some(LocationTable::from_binary(reader,
                                    fnt.maxp_table.ok_or(io::Error::new(io::ErrorKind::Other, "Must load maxp table before loca table!"))?.num_glyphs as usize,
                                    fnt.head_table.ok_or(io::Error::new(io::ErrorKind::Other, "Must load head table before loca table!"))?.index_to_locformat)?);
                },
                TableTag::HorizDevMetric =>
                    fnt.hdmx_table = Some(HorizDeviceMetricsTable::from_binary(reader,
                                    fnt.maxp_table.ok_or(io::Error::new(io::ErrorKind::Other, "Must load maxp table before hdmx table!"))?.num_glyphs as usize)?),
                TableTag::FontHeader =>
                    fnt.head_table = Some({ let v = FontHeader::from_binary(reader)?; /*println!("got head table = {:?}", v);*/ v } ),
                TableTag::MaxProfile => {
                    fnt.maxp_table = Some(MaxProfileTable::from_binary(reader)?);
                    //println!("got maxp table = {:?}", fnt.maxp_table);
                }
                _ =>  { /*println!("Unknown table tag: {:?}!", tde.tag);*/ continue; }
            }
        }
        Ok(fnt)
    }
}

#[cfg(test)]
extern crate svg;

#[cfg(test)]
mod tests {

    use super::*;
    use std::fs::File;

    #[test]
    fn test_tabletag() {
        println!("{:?} = {:X} = {:X}", TableTag::CharGlyphMapping, TableTag::CharGlyphMapping as u32, 0x636D6170);
        assert_eq!(TableTag::CharGlyphMapping as u32, 0x636D6170);
    }

    #[test]
    fn test_loader() {

        //this needs to be changed to be xplat, probably a font in the repo
        let mut font_file = File::open(
            //"/Library/Fonts/Arial.ttf"
            "C:\\Windows\\Fonts\\arial.ttf"
            //"C:\\Windows\\Fonts\\comic.ttf"
            //"FantasqueSansMono-Regular.ttf"
            //"uu.ttf"
            //"test.TTF"
            ).unwrap();

        let f = SfntFont::from_binary(&mut font_file).unwrap();
        println!("SfntFont = {:?}", f);
    }

    #[test]
    fn test_glyph_load_exp_svg() {
        use self::svg::Document;
        use self::svg::Node;
        use self::svg::node::element::{Text, Path, Rectangle, Circle, Group};
        use self::svg::node::element::path::{Data};

        let mut font_file = File::open("C:\\Windows\\Fonts\\arial.ttf").unwrap();
        let font = SfntFont::from_binary(&mut font_file).unwrap();

        /*let GlyphDescription::Simple { 
            num_contours: _, x_max: x_max, x_min: x_min, y_max: y_max, y_min: y_min,
            end_points_of_contours: epoc, instructions: instr,
            points: pnts 
        } = font.glyf_table.unwrap().glyphs[20];*/

        fn wrap(i: usize, start: usize, end: usize) -> usize {
            let len = (end-start);
            if i >= end {
                start + ((i-start) % len)
            } else if i < start {
                start + ((start-i) % len)
            } else { i }
        }

        fn generate_contour(points: &Vec<GlyphPoint>, start: usize, end: usize) -> Data {
            let mut curve = Data::new();
            let mut i = start;
            while !points[i].on_curve { i += 1 }
            curve = curve.move_to((points[i].x, points[i].y)); i+=1;
            while i < end {
                if points[i].on_curve { 
                    curve = curve.line_to((points[i].x,points[i].y)); 
                    i += 1;
                } else {
                    let mut a = points[i];
                    i+=1;
                    let mut b = points[wrap(i, start, end)];
                    if b.on_curve {
                        curve = curve.quadratic_curve_to((a.x, a.y, b.x, b.y));
                    }
                    else {
                        while !b.on_curve {
                            let midx = (a.x + b.x) / 2;
                            let midy = (a.y + b.y) / 2;
                            curve = curve.quadratic_curve_to((a.x, a.y, midx, midy));
                            a = b;
                            i += 1;
                            b = points[wrap(i, start, end)];
                        } 
                        //assert!(a.on_curve);
                        curve = curve.quadratic_curve_to((a.x, a.y, b.x, b.y));
                    }
                    
                }
            }
            curve.close()
        }
        
        let mut doc = Document::new();
        let mut gx : u64 = 0; let mut gy : u64 = 0; let mut limit = 0;
        for (index, g) in font.glyf_table.unwrap().glyphs.iter().take(128).enumerate() {
            match g {
                &GlyphDescription::Simple { 
                    num_contours: _, x_max, x_min, y_max, y_min,
                    end_points_of_contours: ref epoc, instructions: ref instr,
                    points: ref points 
                } => {
                    let mut g = Group::new();
                    g.append(Rectangle::new()
                             .set("x",x_min)
                             .set("y",y_min)
                             .set("width",x_max-x_min)
                             .set("height",y_max-y_min)
                             .set("fill","none")
                             .set("stroke","black")
                             .set("stroke-width",6));
                    g.append(Text::new().set("x",x_min+10).set("y",x_min+10).add(svg::node::Text::new(format!("g{}",index))));
                    for (i,p) in points.iter().enumerate() {
                        g.append(Circle::new()
                                 .set("cx",p.x)
                                 .set("cy",p.y)
                                 .set("r",6)
                                 .set("fill", if p.on_curve { "black" } else { "red" }));
                        g.append(Text::new().set("x",p.x+10).set("y",p.y+10).add(svg::node::Text::new(format!("{}",i))));
                    }
                    let mut last_ep = 0;
                    for &ep in epoc {
                        g.append(Path::new().set("fill","none")
                                 .set("stroke","blue").set("stroke-width",6)
                                 .set("d",generate_contour(&points, last_ep, ep as usize + 1)));
                        last_ep = ep as usize + 1;
                    }
                    doc.append(g.set("transform", format!("translate({} {})", gx, gy)));
                    gx += ((x_max-x_min)*2) as u64;
                    if gx > 30000 {
                        gx = 0;
                        gy += 3000;
                    }limit+=1;
                },
                _ => {} //println!("compound glyph!")
            }
        }

        doc.assign("viewBox", (0, -500, 32000, gy+3000));

        svg::save("glyph_load_test.svg", &doc).unwrap();
    }
}




