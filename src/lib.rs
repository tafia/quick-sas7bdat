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

/// A sas7bdat reader
#[derive(Debug)]
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
    /// word length
    word_len: usize,
    /// offset for start of page
    page_start: usize,
    /// sub header pointer length
    sub_header_len: usize,
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
        let (word_len, page_start, sub_header_len) = if buf[32] == 0x33 {
            (8, 32, 24)
        } else {
            (4, 16, 12)
        };
        let a1 = if buf[35] == 0x33 { 4 } else { 0 };
        debug!("alignments: a1 {}, word len {}", a1, word_len);

        // endianness and pointer size
        let byte_reader = ByteReader::from_bool(buf[37] == 0x01, word_len == 8);

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
        info!("Dataset Name: {}\r\nFile Type {}", dataset_name.trim(), file_type.trim());

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
        //TODO: check if page_len is big enough
        let page_count = byte_reader.read_usize(&buf[204 + a1..204 + a1 + word_len]);

        Ok(Reader {
            inner: read,
            byte_reader: byte_reader,
            encoding: encoding,
            page_len: page_len,
            page_count: page_count,
            word_len: word_len,
            page_start: page_start,
            sub_header_len: sub_header_len,
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

        let page = Page::new(&self, buf)?;

        self.page_num += 1;
        Ok(Some(page))
    }

}

#[derive(Debug)]
enum PageType {
    Meta,
    Amd,
    Mix,
    Data,
}

impl PageType {
    fn from_u16(page_type: u16) -> Result<Self> {
        match page_type {
            0 => Ok(PageType::Meta),
            1024 => Ok(PageType::Amd),
            512 | 640 => Ok(PageType::Mix),
            256 => Ok(PageType::Data),
            t => bail!(format!("Invalid page type {}", t)),
        }
    }

    fn has_sub_header(&self) -> bool {
        match *self {
            PageType::Data => false,
            _ => true,
        }
    }
}

impl ::std::default::Default for PageType {
    fn default() -> Self {
        PageType::Data
    }
}

/// A page of data
#[derive(Debug, Default)]
pub struct Page {
    page_type: PageType,
    block_count: u16,

    // Sub Headers
    // -----------

    // RowSize
    row_len: usize,
    row_count: usize,
    col_count_p1: usize,
    col_count_p2: usize,
    mix_page_row_count: usize,
    lcp: u16,
    lcs: u16,

    // ColumnSize
    col_count: usize,

    // Counts,

    // ColumnText,
    // ColumnName,
    col_names: Vec<String>,
    // ColumnAttributes,
    // FormatAndLabel,
    // ColumnList,
}

impl Page {

    fn new<R>(reader: &Reader<R>, buf: Vec<u8>) -> Result<Page> {
        let start = reader.page_start;
        let page_type = PageType::from_u16(reader.byte_reader.read_u16(&buf[start..start + 2]))?;
        let block_count = reader.byte_reader.read_u16(&buf[start + 2..start + 4]);

        let mut page = Page {
            page_type: page_type,
            block_count: block_count,
            ..Page::default()
        };

        // sub headers
        if page.page_type.has_sub_header() {
            let sub_header_count = reader.byte_reader.read_u16(&buf[start + 4..start + 6]);
            for ch in buf[start + 8..]
                .chunks(reader.sub_header_len)
                .take(sub_header_count as usize)
            {
                let ptr = SubHeaderPtr::new(reader, ch)?;
                if ptr.len > 0 && !ptr.compression.is_truncated() {
                    page.process_sub_header(reader, &buf[ptr.offset..ptr.offset + ptr.len], ptr)?;
                }
            }
        }

        Ok(page)
    }

    fn process_sub_header<R>(&mut self, reader: &Reader<R>, buf: &[u8], ptr: SubHeaderPtr) -> Result<()> {
        let signature = &buf[..reader.word_len];
        match signature {
            b"\xF7\xF7\xF7\xF7" |
            b"\x00\x00\x00\x00\xF7\xF7\xF7\xF7" |
            b"\xF7\xF7\xF7\xF7\x00\x00\x00\x00" |
            b"\xF7\xF7\xF7\xF7\xFF\xFF\xFB\xFE" => self.process_row_size(reader, buf)?,
            b"\xF6\xF6\xF6\xF6" |
            b"\x00\x00\x00\x00\xF6\xF6\xF6\xF6" |
            b"\xF6\xF6\xF6\xF6\x00\x00\x00\x00" |
            b"\xF6\xF6\xF6\xF6\xFF\xFF\xFB\xFE" => self.process_column_size(reader, buf)?,
            b"\x00\xFC\xFF\xFF" |
            b"\xFF\xFF\xFC\x00" |
            b"\x00\xFC\xFF\xFF\xFF\xFF\xFF\xFF" |
            b"\xFF\xFF\xFF\xFF\xFF\xFF\xFC\x00" => self.process_counts(reader, buf)?,
            b"\xFD\xFF\xFF\xFF" |
            b"\xFF\xFF\xFF\xFD" |
            b"\xFD\xFF\xFF\xFF\xFF\xFF\xFF\xFF" |
            b"\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFD" => self.process_column_text(reader, buf)?,
            b"\xFF\xFF\xFF\xFF" |
            b"\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF" => self.process_column_name(reader, buf)?,
            b"\xFC\xFF\xFF\xFF" |
            b"\xFF\xFF\xFF\xFC" |
            b"\xFC\xFF\xFF\xFF\xFF\xFF\xFF\xFF" |
            b"\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFC" => self.process_column_attributes(reader, buf)?,
            b"\xFE\xFB\xFF\xFF" |
            b"\xFF\xFF\xFB\xFE" |
            b"\xFE\xFB\xFF\xFF\xFF\xFF\xFF\xFF" |
            b"\xFF\xFF\xFF\xFF\xFF\xFF\xFB\xFE" => self.process_format_and_label(reader, buf)?,
            b"\xFE\xFF\xFF\xFF" |
            b"\xFF\xFF\xFF\xFE" |
            b"\xFE\xFF\xFF\xFF\xFF\xFF\xFF\xFF" |
            b"\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFE" => self.process_column_list(reader, buf)?,
            v => bail!("Unrecognized sub header signature {:?}", v),
        };
        Ok(())
    }

    fn process_row_size<R>(&mut self, reader: &Reader<R>, buf: &[u8]) -> Result<()> {
        check_size("row size", buf, reader.word_len, 480, 808)?;

        self.row_len = reader.byte_reader.read_usize(&buf[5 * reader.word_len..]);
        self.row_count = reader.byte_reader.read_usize(&buf[6 * reader.word_len..]);
        self.col_count_p1 = reader.byte_reader.read_usize(&buf[9 * reader.word_len..]);
        self.col_count_p2 = reader.byte_reader.read_usize(&buf[10 * reader.word_len..]);
        self.mix_page_row_count = reader.byte_reader.read_usize(&buf[15 * reader.word_len..]);
        let (lcs, lcp) = if reader.word_len == 4 { (354, 378) } else { (682, 706) };
        self.lcs = reader.byte_reader.read_u16(&buf[lcs..lcs + 2]);
        self.lcp = reader.byte_reader.read_u16(&buf[lcp..lcp + 2]);
        Ok(())
    }

    fn process_column_size<R>(&mut self, reader: &Reader<R>, buf: &[u8]) -> Result<()> {
        check_size("column size", buf, reader.word_len, 12, 24)?;
        self.col_count = reader.byte_reader.read_usize(&buf[1 * reader.word_len..]);
        if self.col_count != self.col_count_p1 + self.col_count_p2 {
            warn!("Column count mismatch ({} + {} != {})",
            self.col_count_p1,
            self.col_count_p2,
            self.col_count);
        }
        Ok(())
    }

    fn process_counts<R>(&mut self, _reader: &Reader<R>, _buf: &[u8]) -> Result<()> {
        // unknown purpose
        Ok(())
    }

    fn process_column_text<R>(&mut self, reader: &Reader<R>, buf: &[u8]) -> Result<()> {
        let block_size = reader.byte_reader.read_u16(&buf[reader.word_len..]);
        let start = if reader.word_len == 4 { 16 } else { 20 };
        let comp_name = &buf[start..start + 8];
//         let match comp_name {
//             b"\x00\x00\x00\x00\x00\x00\x00\x00" => self.lcs = 0,
//             _ if self.lcs > 0 => self.lcp = 0,
//             b"SASYZCRL"

        Ok(())
    }

    fn process_column_name<R>(&mut self, reader: &Reader<R>, buf: &[u8]) -> Result<()> {
        Ok(())
    }

    fn process_column_attributes<R>(&mut self, reader: &Reader<R>, buf: &[u8]) -> Result<()> {
        Ok(())
    }

    fn process_format_and_label<R>(&mut self, reader: &Reader<R>, buf: &[u8]) -> Result<()> {
        Ok(())
    }

    fn process_column_list<R>(&mut self, reader: &Reader<R>, buf: &[u8]) -> Result<()> {
        Ok(())
    }

}

fn check_size(name: &str, buf: &[u8], word_len: usize, len32: usize, len64: usize) -> Result<()> {
    match (word_len, buf.len()) {
        (4, 480) | (8, 808) => Ok(()),
        (w, l) => bail!("Invalid {} sub header length ({}) for word len {}", name, l, w),
    }
}

#[derive(Debug)]
struct SubHeaderPtr {
    offset: usize,
    len: usize,
    compression: Compression,
    typ: u8,
}

impl SubHeaderPtr {
    fn new<R>(reader: &Reader<R>, buf: &[u8]) -> Result<Self> {
        let offset = reader.byte_reader.read_usize(&buf[..reader.word_len]);
        let len = reader.byte_reader.read_usize(&buf[reader.word_len..2 * reader.word_len]);
        let compression = Compression::from_u8(buf[2 * reader.word_len])?;
        Ok(SubHeaderPtr {
            offset: offset,
            len: len,
            compression: compression,
            typ: buf[2 * reader.word_len + 1],
        })
    }
}

#[derive(Debug)]
enum Compression {
    Uncompressed,
    Truncated,
    RLE,
}

impl Compression {
    fn from_u8(compression: u8) -> Result<Compression> {
        match compression {
            0 => Ok(Compression::Uncompressed),
            1 => Ok(Compression::Truncated),
            4 => Ok(Compression::RLE),
            c => bail!("Unrecognized compression: {}", c),
        }
    }
    fn is_truncated(&self) -> bool {
        match *self {
            Compression::Truncated => true,
            _ => false,
        }
    }
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
        println!("{:?}", reader);
        assert_eq!(2 + 2, 4);
    }
}
