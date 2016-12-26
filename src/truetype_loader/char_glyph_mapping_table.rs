use std::io;
use std::io::prelude::*;
use std::fmt;
use std::fmt::{Debug};
use std::mem;
use std::fs::File;
use byteorder::{ByteOrder, BigEndian, ReadBytesExt};

use super::*;

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
pub struct CharGlyphMappingTable {
    table_version: u16,
    encoding_tables: Vec<CharGlyphMappingEncodingTable>
}

impl CharGlyphMappingTable {
    pub fn from_binary<R: Read + Seek>(reader: &mut R, table_offset: u64) -> io::Result<CharGlyphMappingTable> {
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
}


