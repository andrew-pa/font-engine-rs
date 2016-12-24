extern crate byteorder;
use std::io;
use std::io::prelude::*;
use std::fmt;
use std::fmt::{Debug};
use std::mem;
use std::fs::File;
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
    fn fooify(&self);
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct HighByteMappingSubheader {
    first_code: u16,
    entry_count: u16,
    id_delta: i16,
    id_range_offset: u16
}

enum CharGlyphMappingEncodingTableFormat {
    ByteEncoding { 
        glyph_ids: [u16; 256]
    },
    HighByteMapping {
        subheader_keys: [u16; 256],
        subheaders: Vec<HighByteMappingSubheader>,
        glyph_indices: Vec<u16>
    },
    SegmentMapToDelta {
        seg_countx2: u16,
        search_range: u16,
        entry_selector: u16,
        range_shift: u16,
        end_count: Vec<u16>,
        reserved_pad: u16,
        start_count: Vec<u16>,
        id_delta: Vec<u16>,
        id_range_offset: Vec<u16>,
        glyph_indices: Vec<u16>
    },
    Trimmed {
        first_code: u16,
        entry_count: u16,
        glyph_indices: Vec<u16>
    }
}
impl Debug for CharGlyphMappingEncodingTableFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CharGlyphMappingEncodingTableFormat::ByteEncoding {..} => write!(f, "ByteEncoding"),
            CharGlyphMappingEncodingTableFormat::HighByteMapping {..} => write!(f, "HighByteMapping"),
            CharGlyphMappingEncodingTableFormat::SegmentMapToDelta {..} => write!(f, "SegmentMapToDelta"),
            CharGlyphMappingEncodingTableFormat::Trimmed {..} => write!(f, "Trimmed")
        }
    }
}

#[derive(Debug)]
struct CharGlyphMappingEncodingTable {
    platform_id: u16,
    platform_encoding_id: u16,
    version: u16,
    subtable: CharGlyphMappingEncodingTableFormat
}

#[repr(C)]
#[derive(Debug)]
struct CharGlyphMappingTable {
    table_version: u16,
    encoding_tables: Vec<CharGlyphMappingEncodingTable>
}

impl CharGlyphMappingTable {
    fn from_binary<R: Read + Seek>(reader: &mut R, table_offset: u64) -> io::Result<CharGlyphMappingTable> {
        let table_version = reader.read_u16::<BigEndian>()?;
        let num_encoding_tables = reader.read_u16::<BigEndian>()?;
        println!("cmap table ver={}, num_tables={}", table_version, num_encoding_tables);
        let mut encoding_tables = Vec::new();
        for i in 0..num_encoding_tables {
            reader.seek(io::SeekFrom::Start(table_offset + 4 + (8*i) as u64));
            let plat_id = reader.read_u16::<BigEndian>()?;
            let plat_encode_id = reader.read_u16::<BigEndian>()?;
            let offset = reader.read_u32::<BigEndian>()?;
            reader.seek(io::SeekFrom::Start(offset as u64 + table_offset));
            let format = reader.read_u16::<BigEndian>()?;
            let length = reader.read_u16::<BigEndian>()?;
            let ver = reader.read_u16::<BigEndian>()?; 
            println!("font data for table {}: offset={:X}h -> {:X}h; platid={}; plateid={}; version={}; format={}; len={}", i, offset, offset as u64 + table_offset, plat_id, plat_encode_id, ver, format, length);
            encoding_tables.push(
                CharGlyphMappingEncodingTable {
                    platform_id: plat_id,
                    platform_encoding_id: plat_encode_id,
                    version: ver,
                    subtable: match format {
                        0 => {
                            let mut glyph_ids = [0u16; 256];
                            for i in 0..256 {
                                glyph_ids[i] = reader.read_u16::<BigEndian>()?;
                            }
                            CharGlyphMappingEncodingTableFormat::ByteEncoding { glyph_ids: glyph_ids }
                        },
                        2 => {
                            return Err(io::Error::new(io::ErrorKind::Other, "Format 2 Unimplemented"));
                        },
                        4 => {
                            let segcount2 = reader.read_u16::<BigEndian>()?;
                            let segcount = (segcount2 / 2) as usize;
                            let search_range = reader.read_u16::<BigEndian>()?;
                            let entry_selector = reader.read_u16::<BigEndian>()?;
                            let range_shift = reader.read_u16::<BigEndian>()?;
                            let mut end_count = Vec::with_capacity(segcount);
                            for i in 0..segcount {
                                end_count.push(reader.read_u16::<BigEndian>()?);
                            }
                            reader.read_u16::<BigEndian>()?; //skip reserved padding u16
                            let mut start_count = Vec::with_capacity(segcount);
                            for i in 0..segcount {
                                start_count.push(reader.read_u16::<BigEndian>()?);
                            }
                            let mut id_delta = Vec::with_capacity(segcount);
                            for i in 0..segcount {
                                id_delta.push(reader.read_u16::<BigEndian>()?);
                            }
                            let mut id_range_offset = Vec::with_capacity(segcount);
                            for i in 0..segcount {
                                id_range_offset.push(reader.read_u16::<BigEndian>()?);
                            }
                            let glyph_indices_count = (length as usize - (2 * (8 + 4*segcount))) / 2; 
                            let mut glyph_indices = Vec::with_capacity(glyph_indices_count);
                            for i in 0..glyph_indices_count {
                                glyph_indices.push(reader.read_u16::<BigEndian>()?);
                            }
                            CharGlyphMappingEncodingTableFormat::SegmentMapToDelta {
                                seg_countx2: segcount2,
                                search_range: search_range,
                                entry_selector: entry_selector,
                                range_shift: range_shift,
                                reserved_pad: 0,
                                start_count: start_count,
                                end_count: end_count,
                                id_delta: id_delta,
                                id_range_offset: id_range_offset,
                                glyph_indices: glyph_indices
                            }
                        },
                        6 => {
                            let first_code = reader.read_u16::<BigEndian>()?;
                            let entry_count = reader.read_u16::<BigEndian>()?;
                            let mut glyph_indices = Vec::with_capacity(entry_count as usize);
                            for i in 0..entry_count {
                                glyph_indices.push(reader.read_u16::<BigEndian>()?);
                            }
                            CharGlyphMappingEncodingTableFormat::Trimmed {
                                first_code: first_code,
                                entry_count: entry_count,
                                glyph_indices: glyph_indices
                            }
                        },
                        _ => return Err(io::Error::new(io::ErrorKind::Other, "Unknown Format"))
                    }
                });
        }
        Ok(CharGlyphMappingTable{table_version:table_version, encoding_tables:encoding_tables})
    }
}

impl Table for CharGlyphMappingTable {
    fn tag(&self) -> TableTag { TableTag::CharGlyphMapping }
    fn fooify(&self) { println!("foo: {}", self.table_version); }
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
    tables: Vec<Box<Table>>
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
            table_directory.push(TableDirectoryEntry::from_binary(reader)?);
        }
        println!("table directory: {:?}", table_directory);
        let mut tables = Vec::<Box<Table>>::new();
        for tde in &table_directory {
            reader.seek(io::SeekFrom::Start(tde.offset as u64));
            match tde.tag {
                TableTag::CharGlyphMapping => {
                    tables.push(Box::new(CharGlyphMappingTable::from_binary(reader, tde.offset as u64)?));
                },
                _ => println!("Unknown table tag: {:?}!", tde.tag)
            }
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
    use truetype_file::*;

    #[test]
    fn test_tabletag() {
        println!("{:?} = {:X} = {:X}", TableTag::CharGlyphMapping, TableTag::CharGlyphMapping as u32, 0x636D6170);
        assert_eq!(TableTag::CharGlyphMapping as u32, 0x636D6170);
    }

    #[test]
    fn test_header() {

        //this needs to be changed to be xplat, probably a font in the repo
        let mut font_file = File::open("C:\\Windows\\Fonts\\arial.ttf").unwrap();

        let otbl = OffsetTable::from_binary(&mut font_file).unwrap();
        println!("OffsetTable = {:?}", otbl);
    }
}




