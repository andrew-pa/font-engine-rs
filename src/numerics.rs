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

impl Into<u32> for F26d6 {
    fn into(self) -> u32 {
        self.0 as u32
    }
}

impl Into<f32> for F26d6 {
    fn into(self) -> f32 {
        self.0 as f32
    }
}

impl F26d6 {
    pub fn abs(self) -> F26d6 {
        F26d6(self.0.abs())
    }

    pub fn floor(self) -> F26d6 {
        F26d6(self.0 & 0xffff_ffc0)
    }

    pub fn ceil(self) -> F26d6 {
        F26d6(self.0 & 0xffff_ffc0)
    }
}

use std::ops::*;

impl Add<F26d6> for F26d6 {
    type Output = F26d6;
    fn add(self, othr: F26d6) -> F26d6 {
        F26d6(self.0 + othr.0)
    }
}
impl Sub<F26d6> for F26d6 {
    type Output = F26d6;
    fn sub(self, othr: F26d6) -> F26d6 {
        F26d6(self.0 - othr.0)
    }
}
impl Mul<F26d6> for F26d6 {
    type Output = F26d6;
    fn mul(self, othr: F26d6) -> F26d6 {
        F26d6(self.0 * othr.0)
    }
}
impl Div<F26d6> for F26d6 {
    type Output = F26d6;
    fn div(self, othr: F26d6) -> F26d6 {
        F26d6(self.0 / othr.0)
    }
}

impl Neg for F26d6 {
    type Output = F26d6;
    fn neg(self) -> F26d6 {
        F26d6(-self.0)
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


