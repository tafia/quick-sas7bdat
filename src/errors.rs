//! A module to handle all errors via error-chain crate

#![allow(missing_docs)]

error_chain! {
    foreign_links {
        Io(::std::io::Error);
        StrUtf8(::std::str::Utf8Error);
    }
    errors {
        Invalid(name: &'static str) {
            description("invalid data")
            display("invalid {}", name)
        }
        OsType(typ: u8) {
            description("unrecognized OS type")
            display("expecting 0x01 or 0x02, got {}", typ)
        }
    }
}
