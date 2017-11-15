//! A module to abstract endianness

use byteorder::{ByteOrder, LittleEndian, BigEndian};
use std::fmt;

macro_rules! declare_endian {
    ($($f:ident, $t:ty),*) => {

/// An Endian wrapper
pub struct Endian {
    $(
    $f: &'static Fn(&[u8]) -> $t,
    )*
}

impl fmt::Debug for Endian {
    fn fmt(&self, _f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        Ok(())
    }
}

impl Endian {
        $(
    pub fn $f(&self, buf: &[u8]) -> $t {
        (self.$f)(buf)
    }
        )*

    /// Creates a new Endian wrapper
    pub fn from_bool(is_big_endian: bool) -> Endian {
        macro_rules! make_endian {
            ($e:ident) => {
                Endian {
                    $(
                    $f: &$e::$f,
                    )*
                }
            }
        }
        if is_big_endian {
            make_endian!(BigEndian)
        } else {
            make_endian!(LittleEndian)
        }
    }
}

    }
}

declare_endian!(read_i32, i32,
                read_i64, i64);

