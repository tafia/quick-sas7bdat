//! A module to abstract endianness

use byteorder::{ByteOrder, LittleEndian, BigEndian};
use std::fmt;

macro_rules! declare_reader {
    ($($f:ident, $t:ty),*) => {

/// An byte reader wrapper
pub struct ByteReader {
    read_isize: Box<Fn(&[u8]) -> isize>,
    read_usize: Box<Fn(&[u8]) -> usize>,
    $(
    $f: &'static Fn(&[u8]) -> $t,
    )*
}

impl fmt::Debug for ByteReader {
    fn fmt(&self, _f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        Ok(())
    }
}


impl ByteReader {
        $(
    pub fn $f(&self, buf: &[u8]) -> $t {
        (self.$f)(buf)
    }
        )*

    /// Creates a new Endian wrapper
    pub fn from_bool(is_little_endian: bool, is_64: bool) -> Self {
        macro_rules! make_reader {
            ($e:ident) => {
                if is_64 {
                    ByteReader {
                        read_isize: Box::new(|buf| $e::read_i64(buf) as isize),
                        read_usize: Box::new(|buf| $e::read_u64(buf) as usize),
                        $(
                        $f: &$e::$f,
                        )*
                    }
                } else {
                    ByteReader {
                        read_isize: Box::new(|buf| $e::read_i32(buf) as isize),
                        read_usize: Box::new(|buf| $e::read_u32(buf) as usize),
                        $(
                        $f: &$e::$f,
                        )*
                    }
                }
            }
        }
        if is_little_endian {
            make_reader!(LittleEndian)
        } else {
            make_reader!(BigEndian)
        }
    }

    pub fn read_isize(&self, buf: &[u8]) -> isize {
        (self.read_isize)(buf)
    }

    pub fn read_usize(&self, buf: &[u8]) -> usize {
        (self.read_usize)(buf)
    }
}

    }
}

declare_reader!(read_i32, i32,
                read_u16, u16,
                read_i64, i64);
