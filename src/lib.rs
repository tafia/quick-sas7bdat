//! A SAS7BDAT binary file reader

#![deny(missing_docs)]

#[macro_use] extern crate error_chain;
extern crate byteorder;
#[macro_use] extern crate log;
extern crate encoding_rs;

pub mod errors;
mod byte_reader;

use errors::{Result, ErrorKind};
use std::io::{Read};
use byte_reader::ByteReader;
use encoding_rs::Encoding;

#[derive(Debug)]
struct Offsets {
    int: usize,
    page_start: usize,
    sub_header_ptr_len: usize,
}

/// A sas7bdat reader
pub struct Reader<R> {
    /// The inner reader
    inner: R,
    /// current page
    page_num: usize,
    /// a byte reader, taking endianness and word length into account
    byte_reader: ByteReader,
    /// encoding to get utf8 &str
    encoding: &'static Encoding,
    /// page length
    page_len: usize,
    /// total number of pages in the document
    page_count: usize,
    /// various offset depending on word length
    offsets: Offsets,
}

impl<R: Read> Reader<R> {

    /// Creates a new sas7bdat `Reader` out of a regular reader
    ///
    /// Initialize by reading the header
    pub fn from_reader(mut read: R) -> Result<Self> {
        let mut buf = [0u8; 1024];
        read.read_exact(&mut buf[0..1024])?;
        
        // magic number
        if &buf[..32] != &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0xc2, 0xea, 0x81, 0x60,
            0xb3, 0x14, 0x11, 0xcf, 0xbd, 0x92, 0x08, 0x00,
            0x09, 0xc7, 0x31, 0x8c, 0x18, 0x1f, 0x10, 0x11] {
            bail!(ErrorKind::Invalid("Magic Number"));
        }

        // alignments
        let offsets = if buf[32] == 0x33 {
            Offsets {
                int: 4,
                page_start: 32,
                sub_header_ptr_len: 24,
            }
        } else {
            Offsets {
                int: 0,
                page_start: 16,
                sub_header_ptr_len: 12,
            }
        };
        let a1 = if buf[35] == 0x33 { 4 } else { 0 };
        debug!("alignments: a1 {}, a2 {}", a1, offsets.int);

        // endianness and pointer size
        let byte_reader = ByteReader::from_bool(buf[37] == 0x01, offsets.int == 4);

        if &buf[84..92] != b"SAS FILE" {
            bail!(ErrorKind::Invalid("SAS FILE"));
        }

        let encoding = Encoding::for_label(match buf[70] {
            29 => b"latin1",
            20 => b"utf-8",
            33 => b"cyrillic",
            60 => b"wlatin2",
            61 => b"wcyrillic",
            62 => b"wlatin1",
            90 => b"ebcdic870",
            v => bail!("Unknown encoding {}", v),
        }).unwrap_or(::encoding_rs::UTF_8);

        let dataset_name = encoding.decode(&buf[92..156]).0;
        let file_type = encoding.decode(&buf[156..164]).0;
        debug!("{} {}", dataset_name, file_type);

        let header_len = byte_reader.read_i32(&buf[(196 + a1)..(200 + a1)]);
        match header_len {
            1024 => (),
            8192 => {
                // exhaust header
                let mut remaining = [0; 8192 - 1024];
                read.read_exact(&mut remaining)?;
            }
            l => bail!(format!("Invalid header: {}", l)),
        }

        let page_len = byte_reader.read_i32(&buf[200 + a1..204 + a1]) as usize;
        let page_count = byte_reader.read_usize(&buf[204 + a1..208 + a1 + offsets.int]);

        Ok(Reader {
            inner: read,
            byte_reader: byte_reader,
            encoding: encoding,
            page_len: page_len,
            page_count: page_count,
            offsets: offsets,
            page_num: 0,
        })
    }

    /// Reads the next page
    pub fn next_page(&mut self) -> Result<Option<Page>> {
        if self.page_num == self.page_count {
            return Ok(None);
        }

        // fill buffer
        let mut buf = vec![0u8; self.page_len];
        self.inner.read_exact(&mut buf)?;

        let byte_reader = &self.byte_reader;
        let start = self.offsets.page_start;
        let page_type = byte_reader.read_u16(&buf[start..]);

        let page = Page {};
        self.page_num += 1;
        Ok(Some(page))
    }

}

/// A page of data
pub struct Page {

}


#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::{BufReader};
    use super::*;

    fn get_file() -> BufReader<File> {
        BufReader::new(File::open("tests/samples/olympic.sas7bdat").unwrap())
    }

    #[test]
    fn it_works() {
        let reader = Reader::from_reader(get_file()).unwrap();
        println!("{:?}", reader.header);
        assert_eq!(2 + 2, 4);
    }
}
