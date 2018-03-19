use std::io::{Read, Result as IOResult};
use byteorder::{ByteOrder, BigEndian, ReadBytesExt};

use fix;
use typenum;

pub type F2dot14 = fix::aliases::binary::IFix16<typenum::N14>;

pub struct F26d6(i32);

impl From<i32> for F26d6 {
    fn from(v: i32) -> F26d6 {
        F26d6(v << 6)
    }
}

impl From<f32> for F26d6 {
    fn from(v: f32) -> F26d6 {
        let i = v.floor() as i32;
        let f = (v.fract().abs() * 64.0).ceil() as i32;
        F26d6(i << 6 | (f & 0x0000_004f))
    }
}

#[cfg(test)]
mod tests {
}

/*#[derive(Copy, Clone, Debug)]
pub struct F2dot14(u16);

impl From<u16> for F2dot14 {
    fn from(v: u16) -> F2dot14 {
        F2dot14(v)
    }
}

//add, sub, mul, div

#[derive(Copy, Clone, Debug)]
pub struct F26dot6(i32);*/


