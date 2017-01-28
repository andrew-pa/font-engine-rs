use std::io;
use std::io::prelude::*;
use std::fmt;
use std::fmt::{Debug};
use std::mem;
use std::fs::File;
use std::rc::Rc;
use byteorder::{ByteOrder, BigEndian, ReadBytesExt};

/*
 * TODO: Move some of this code around so it's in more than one file and this is a bigger module
 *       CMAP table code can probably go in it's own file
 */

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

#[derive(Copy, Clone, Debug)]
struct fixed {
    int_part: u16,
    frac_part: u16
}
impl fixed {
    fn from_binary<R: Read + Seek, E: ByteOrder>(r: &mut R) -> io::Result<fixed> {
        Ok(fixed{ int_part: r.read_u16::<E>()?, frac_part: r.read_u16::<E>()? })
    }
}
#[derive(Copy, Clone, Debug)]
struct f2dot14(i16);

macro_rules! table_tag_code {
    ($a:expr, $b:expr, $c:expr, $d:expr) => (($a as u32) << 24 | ($b as u32) << 16 | ($c as u32) << 8 | ($d as u32));
}

#[repr(u32)]
#[derive(Copy, Clone)]
enum TableTag {
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

trait Table : Debug {
    fn tag(&self) -> TableTag;
}

mod char_glyph_mapping_table;
use self::char_glyph_mapping_table::*;
mod glyph_data_table;
use self::glyph_data_table::*;

struct ControlValueTable(Vec<i16>);

impl Table for ControlValueTable {
    fn tag(&self) -> TableTag { TableTag::ControlValue }
}

impl Debug for ControlValueTable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ControlValueTable [len = {}]", self.0.len())
    }
}

struct FontProgram(Vec<u8>);

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
        const GASP_Neither = 0x0000u16,
        const GASP_GridFit = 0x1000u16,    //these are tricky because it's in big endian format
        const GASP_Grayscale = 0x2000u16,
    }
}

#[derive(Copy, Clone, Debug)]
struct GASPRange {
    range_max_PPEM: u16,
    range_gasp_behavior: u16
}

#[derive(Debug)]
struct GASPTable {
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
        for i in 0..num_ranges {
            let gb = reader.read_u16::<BigEndian>()?;
            println!("GASP bits {:b}b ; {:b}b", gb, GASP_GridFit.bits());
            r.push(GASPRange {
                range_max_PPEM: reader.read_u16::<BigEndian>()?,
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
struct HorizDeviceMetricsTable {
    version: u16,
    records: Vec<DeviceRecord>
}

impl HorizDeviceMetricsTable {
    fn from_binary<R: Read+Seek>(reader: &mut R, num_glyphs: usize) -> io::Result<HorizDeviceMetricsTable> {
        let v = reader.read_u16::<BigEndian>()?;
        let num_dr = reader.read_i16::<BigEndian>()?;
        let size_dr = reader.read_i32::<BigEndian>()?;
        let mut r = Vec::new();
        for i in 0..num_dr {
            let ps = reader.read_u8()?;
            let mw = reader.read_u8()?;
            let mut w = vec![0u8; num_glyphs];
            reader.read_exact(w.as_mut_slice())?;
            r.push(DeviceRecord::Format0 {
                pixel_size: ps,
                max_width: mw,
                widths: w 
            }); // this requires knowing numGlyphs from the maxp table
            reader.seek(io::SeekFrom::Current(size_dr as i64));
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
struct FontHeader {
    version: fixed,
    font_rev: fixed,
    checksum: u32,
    flags: u16,
    units_per_em: u16,
    created: u64,
    modified: u64,
    x_min: i16,
    y_min: i16,
    x_max: i16,
    y_max: i16,
    mac_style: u16,
    lowest_rec_ppem: u16,
    font_direction_hint: i16,
    index_to_locformat: i16,
    glyph_data_format: i16
}

impl FontHeader {
    fn from_binary<R: Read + Seek>(reader: &mut R) -> io::Result<FontHeader> {
        Ok(FontHeader {
            version: fixed::from_binary::<R,BigEndian>(reader)?,
            font_rev: fixed::from_binary::<R,BigEndian>(reader)?,
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
    version: fixed,
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
            version: fixed::from_binary::<R,BigEndian>(reader)?,
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
                for i in 0..(num_glyphs+1) {
                    v.push(if format == 1 { reader.read_u32::<BigEndian>()? } else { reader.read_u16::<BigEndian>()? as u32 *2 }) 
                }
                v
            }
        })
    }
}


#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct TableDirectoryEntry {
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
struct OffsetTable {
    sfnt_version: fixed,
    search_range: u16,
    entry_selector: u16,
    range_shift: u16,
    table_directory: Vec<TableDirectoryEntry>,
    tables: Vec<Rc<Table>>
}

impl OffsetTable {
    fn from_binary<R: Read + Seek>(reader : &mut R) -> io::Result<OffsetTable> {
        let version = fixed::from_binary::<R,BigEndian>(reader)?;
        let num_tables = reader.read_u16::<BigEndian>()?;
        let search_range = reader.read_u16::<BigEndian>()?;
        let entry_sel = reader.read_u16::<BigEndian>()?;
        let range_shift = reader.read_u16::<BigEndian>()?;
        let mut table_directory = Vec::new();
        for i in 0..num_tables {
            let tbe = TableDirectoryEntry::from_binary(reader)?;
            match tbe.tag {
                TableTag::MaxProfile => table_directory.insert(0, tbe),
                TableTag::FontHeader => table_directory.insert(1, tbe),
                TableTag::LocationIndex => table_directory.insert(2, tbe),
                _ => table_directory.push(tbe)
            }
        }
        println!("table directory: {:?}", table_directory);
        let mut tables = Vec::<Rc<Table>>::new();
        let mut maxp_table : Option<MaxProfileTable> = None;
        let mut head_table : Option<FontHeader> = None;
        let mut loca_table : Option<Rc<LocationTable>> = None;
        for tde in &table_directory {
            reader.seek(io::SeekFrom::Start(tde.offset as u64));
            tables.push(match tde.tag {
                TableTag::CharGlyphMapping =>
                   Rc::new(char_glyph_mapping_table::CharGlyphMappingTable::from_binary(reader, tde.offset as u64)?),
                TableTag::ControlValue => {
                    let mut tbl = Vec::with_capacity((tde.length/2) as usize);
                    for i in 0..tde.length {
                        tbl.push(reader.read_i16::<BigEndian>()?);
                    }
                    Rc::new(ControlValueTable(tbl))
                },
                TableTag::FontProgram => {
                    let mut tbl = vec![0u8; tde.length as usize];
                    reader.read_exact(tbl.as_mut_slice())?;
                    Rc::new(FontProgram(tbl))
                },
                TableTag::GridFitAndScanConvertProc => 
                    Rc::new(GASPTable::from_binary(reader)?),
                TableTag::GlyphData =>
                    Rc::new(GlyphDataTable::from_binary(reader, tde.offset as u64, 
                                    maxp_table.ok_or(io::Error::new(io::ErrorKind::Other, "Must load maxp table before glyf table!"))?,
                                    loca_table.clone().ok_or(io::Error::new(io::ErrorKind::Other, "Must load loca table before glyf table!"))?)?),
                TableTag::LocationIndex => {
                    loca_table = Some(Rc::new(LocationTable::from_binary(reader, 
                                    maxp_table.ok_or(io::Error::new(io::ErrorKind::Other, "Must load maxp table before loca table!"))?.num_glyphs as usize,
                                    head_table.ok_or(io::Error::new(io::ErrorKind::Other, "Must load head table before loca table!"))?.index_to_locformat)?));
                    loca_table.clone().unwrap()
                },
                TableTag::HorizDevMetric =>
                    Rc::new(HorizDeviceMetricsTable::from_binary(reader, 
                                    maxp_table.ok_or(io::Error::new(io::ErrorKind::Other, "Must load maxp table before hdmx table!"))?.num_glyphs as usize)?),
                TableTag::FontHeader => 
                    Rc::new({ let v = FontHeader::from_binary(reader)?; println!("got head table = {:?}", v); head_table = Some(v); v }),
                TableTag::MaxProfile => {
                    let mpt = MaxProfileTable::from_binary(reader)?; 
                    maxp_table = Some(mpt);
                    println!("got maxp table = {:?}", mpt);
                    Rc::new(mpt)
                }
                _ =>  { println!("Unknown table tag: {:?}!", tde.tag); continue; }
            })
        }
        Ok(OffsetTable {
            sfnt_version: version,
            search_range: search_range,
            entry_selector: entry_sel,
            range_shift: range_shift,
            table_directory: table_directory,
            tables: tables
        })
    }
}

fn calculate_table_checksum() -> u32 { 0 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tabletag() {
        println!("{:?} = {:X} = {:X}", TableTag::CharGlyphMapping, TableTag::CharGlyphMapping as u32, 0x636D6170);
        assert_eq!(TableTag::CharGlyphMapping as u32, 0x636D6170);
    }

    #[test]
    fn test_header() {

        //this needs to be changed to be xplat, probably a font in the repo
        let mut font_file = File::open(
            //"C:\\Windows\\Fonts\\arial.ttf"
            "C:\\Windows\\Fonts\\comic.ttf"
            //"FantasqueSansMono-Regular.ttf"
            //"uu.ttf"
            //"test.TTF"
            ).unwrap();

        let otbl = OffsetTable::from_binary(&mut font_file).unwrap();
        println!("OffsetTable = {:?}", otbl);
    }
}




