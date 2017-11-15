//! A SAS7BDAT binary file reader

#![deny(missing_docs)]

#[macro_use] extern crate error_chain;
extern crate byteorder;
#[macro_use] extern crate log;

pub mod errors;
mod endian;

use errors::{Result, ErrorKind};
use std::io::{Read};
use std::str::from_utf8;
use endian::Endian;

#[derive(Debug)]
enum OsType {
    Unix,
    Win,
}

#[derive(Debug)]
struct Header {
     endian: Endian,
     page_len: usize,
     page_count: usize,
     is_64: bool,
}


impl Header {

    /// Parses bytes into a new Header
    fn from_reader<R: Read>(read: &mut R) -> Result<Header> {

        // maximum relevant information is 342 bytes
        let mut buf = [0u8; 342];

        // first chunk of data, alignment free
        read.read_exact(&mut buf[0..164])?;
        
        // magic number
        if &buf[..32] != &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0xc2, 0xea, 0x81, 0x60,
            0xb3, 0x14, 0x11, 0xcf, 0xbd, 0x92, 0x08, 0x00,
            0x09, 0xc7, 0x31, 0x8c, 0x18, 0x1f, 0x10, 0x11] {
            bail!(ErrorKind::Invalid("Magic Number"));
        }

        // alignments
        let a2 = if buf[32] == 0x33 { 4 } else { 0 };
        let a1 = if buf[35] == 0x33 { 4 } else { 0 };

        // endianness
        let endian = Endian::from_bool(buf[37] == 0x00);

        // os type
        let os_type = match buf[39] {
            0x01 => OsType::Unix,
            0x02 => OsType::Win,
            t => bail!(ErrorKind::OsType(t)),
        };
        debug!("{:?}", os_type);

        if &buf[84..92] != b"SAS FILE" {
            bail!(ErrorKind::Invalid("SAS FILE"));
        }

        let dataset_name = from_utf8(&buf[92..156])?;
        let file_type = from_utf8(&buf[156..164])?;

        let header_len = endian.read_i32(&buf[196 + a1..200 + a1]);
        match header_len {
            1024 | 8192 => (),
            l => bail!(ErrorKind::Invalid("Header Length")),
        }

        let page_len = endian.read_i32(&buf[200 + a1..204 + a1]) as usize;
        let page_count = if a2 == 0 {
            endian.read_i32(&buf[204 + a1..208 + a1 + a2]) as usize
        } else {
            endian.read_i64(&buf[204 + a1..208 + a1 + a2]) as usize
        };

        // exhaust header
        let mut remaining = vec![0; header_len as usize - 342];
        read.read_exact(&mut remaining)?;

        Ok(Header {
            endian: endian,
            page_len: page_len,
            page_count: page_count,
            is_64: a2 == 4,
        })
    }
}


/// A sas7bdat reader
pub struct Reader<R> {
    /// The inner reader
    inner: R,
    /// The file header
    header: Header,
}

impl<R: Read> Reader<R> {

    /// Creates a new sas7bdat `Reader` out of a regular reader
    pub fn from_reader(mut reader: R) -> Result<Self> {

        let header = Header::from_reader(&mut reader)?;

        Ok(Reader {
            inner: reader,
            header: header,
        })
    }

}




#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
