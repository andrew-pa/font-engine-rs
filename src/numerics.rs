use std::io::{Read, Result as IOResult};
use byteorder::{ByteOrder, BigEndian, ReadBytesExt};

use fix;
use typenum;

pub type F2dot14 = fix::aliases::binary::IFix16<typenum::N14>;
pub type F26dot6 = fix::aliases::binary::IFix32<typenum::N6>;

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


