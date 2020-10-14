
//
// FSUIPC library
// Copyright (c) 2015 Alvaro Polo
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::io;
use std::io::{Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use super::raw::RawBytes;

#[cfg(all(windows, target_pointer_width = "32"))]
pub type WinUInt = u32;
#[cfg(all(windows, target_pointer_width = "32"))]
pub type WinInt = i32;

#[cfg(all(windows, target_pointer_width = "64"))]
pub type WinUInt = u64;
#[cfg(all(windows, target_pointer_width = "64"))]
pub type WinInt = i64;

/// The header of a message sent to FSUIPC module via IPC
#[derive(Debug, PartialEq)]
pub enum MsgHeader {
    /// Read state data message header
    /// Read `len` bytes from given offset and prepar to store `data` in `target`.
    ReadStateData {
        offset: u16,
        len: usize,
        target: *mut u8,
    },
    /// Write state data message header
    /// Write `len` bytes from given `source` to given offset.
    WriteStateData {
        offset: u16,
        len: usize,
    },
    TerminationMark
}

pub trait MsgRead : Read {
    /// Read a IPC message header from the given `Read` object.
    /// It returns the read message header and the number of bytes processed.
    fn read_header(&mut self) -> io::Result<MsgHeader> {
        match self.read_u32::<LittleEndian>()? {
            FS6IPC_READSTATEDATA_ID => {
                let offset = self.read_u32::<LittleEndian>()? as u16;
                let len = self.read_u32::<LittleEndian>()? as usize;
                let target = self.read_u32::<LittleEndian>()? as *mut u8;
                Ok(MsgHeader::ReadStateData {
                    offset: offset,
                    len: len,
                    target: target,
                })
            },
            FS6IPC_WRITESTATEDATA_ID => {
                let offset = self.read_u32::<LittleEndian>()? as u16;
                let len = self.read_u32::<LittleEndian>()? as usize;
                Ok(MsgHeader::WriteStateData {
                    offset: offset,
                    len: len,
                })
            },
            FS6IPC_TERMINATIONMARK_ID => return Ok(MsgHeader::TerminationMark),
            unexpected => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected double word 0x{} while reading IPC message header",
                    unexpected))),
        }
    }

    fn read_body<W: Write>(&mut self, header: &MsgHeader, output: &mut W) -> io::Result<usize> {
        match header {
            &MsgHeader::ReadStateData { offset: _, len, target: _ } => {
                for _ in 0..len { output.write_u8(self.read_u8()?)?; }
                Ok(len)
            },
            &MsgHeader::WriteStateData { offset: _, len } => {
                for _ in 0..len { output.write_u8(self.read_u8()?)?; }
                Ok(len)
            },
            &MsgHeader::TerminationMark => Ok(0),
        }
    }
}

impl<R: Read + ?Sized> MsgRead for R {}

pub trait MsgWrite : Write {
    /// Write a IPC message header into the given `Write` object.
    fn write_header(&mut self, msg: &MsgHeader) -> io::Result<usize> {
        match msg {
            &MsgHeader::ReadStateData { offset, len, target } => {
                self.write_u32::<LittleEndian>(FS6IPC_READSTATEDATA_ID)?;
                self.write_u32::<LittleEndian>(offset as u32)?;
                self.write_u32::<LittleEndian>(len as u32)?;
                self.write_u32::<LittleEndian>(target as u32)?;
                Ok(16)
            },
            &MsgHeader::WriteStateData { offset, len } => {
                self.write_u32::<LittleEndian>(FS6IPC_WRITESTATEDATA_ID)?;
                self.write_u32::<LittleEndian>(offset as u32)?;
                self.write_u32::<LittleEndian>(len as u32)?;
                Ok(12)
            },
            &MsgHeader::TerminationMark => {
                self.write_u32::<LittleEndian>(FS6IPC_TERMINATIONMARK_ID)?;
                Ok(4)
            },
        }
    }

    fn write_body<R: Read>(&mut self, header: &MsgHeader, input: &mut R) -> io::Result<usize> {
        match header {
            &MsgHeader::ReadStateData { offset: _, len, target: _ } => {
                for _ in 0..len { self.write_u8(input.read_u8()?)?; }
                Ok(len)
            },
            &MsgHeader::WriteStateData { offset: _, len } => {
                for _ in 0..len { self.write_u8(input.read_u8()?)?; }
                Ok(len)
            },
            &MsgHeader::TerminationMark => Ok(0),
        }
    }

    fn write_rsd(&mut self, offset: u16, dest: *mut u8, len: usize) -> io::Result<usize> {
        let header = MsgHeader::ReadStateData {
            offset: offset, len: len, target: dest,
        };
        let hdr_bytes = self.write_header(&header)?;
        let body_bytes = self.write_body(&header, &mut io::repeat(0))?;
        Ok(hdr_bytes + body_bytes)
    }

    fn write_wsd(&mut self, offset: u16, src: *const u8, len: usize) -> io::Result<usize> {
        let header = MsgHeader::WriteStateData {
            offset: offset, len: len,
        };
        let hdr_bytes = self.write_header(&header)?;
        let body_bytes = self.write_body(&header, &mut RawBytes::new(src, len))?;
        Ok(hdr_bytes + body_bytes)
    }
}

impl<W: Write + ?Sized> MsgWrite for W {}

const FS6IPC_TERMINATIONMARK_ID: u32 = 0;
const FS6IPC_READSTATEDATA_ID: u32 = 1;
const FS6IPC_WRITESTATEDATA_ID: u32 = 2;

#[cfg(test)]
mod test {

    use std::io::{Cursor, ErrorKind};

    use byteorder::{LittleEndian, ReadBytesExt};

    use super::*;

    #[test]
    fn should_read_rsd_header() {
        let mut buff: &[u8] = &[
            0x01, 0x00, 0x00, 0x00,
            0x00, 0x10, 0x00, 0x00,
            0x04, 0x00, 0x00, 0x00,
            0x00, 0x20, 0x00, 0x00,
        ];
        let expected = MsgHeader::ReadStateData {
            offset: 0x1000,
            len: 4,
            target: 0x2000 as *mut u8,
        };
        assert_eq!(buff.read_header().unwrap(), expected);
    }

    #[test]
    fn should_read_rsd_body() {
        let mut buff: &[u8] = &[ 0x01, 0x02, 0x03, 0x04 ];
        let header = MsgHeader::ReadStateData {
            offset: 0x1000,
            len: 4,
            target: 0x2000 as *mut u8,
        };
        let mut data = Vec::new();
        assert_eq!(buff.read_body(&header, &mut data).unwrap(), 4);
        assert_eq!(data[0], 1);
        assert_eq!(data[1], 2);
        assert_eq!(data[2], 3);
        assert_eq!(data[3], 4);
    }

    #[test]
    fn should_read_wsd_header() {
        let mut buff: &[u8] = &[
            0x02, 0x00, 0x00, 0x00,
            0x00, 0x10, 0x00, 0x00,
            0x04, 0x00, 0x00, 0x00,
        ];
        let expected = MsgHeader::WriteStateData {
            offset: 0x1000,
            len: 4,
        };
        assert_eq!(buff.read_header().unwrap(), expected);
    }

    #[test]
    fn should_read_wsd_body() {
        let mut buff: &[u8] = &[ 0x01, 0x02, 0x03, 0x04 ];
        let header = MsgHeader::WriteStateData {
            offset: 0x1000,
            len: 4,
        };
        let mut data = Vec::new();
        assert_eq!(buff.read_body(&header, &mut data).unwrap(), 4);
        assert_eq!(data[0], 1);
        assert_eq!(data[1], 2);
        assert_eq!(data[2], 3);
        assert_eq!(data[3], 4);
    }

    #[test]
    fn should_read_tm_header() {
        let mut buff: &[u8] = &[0x00, 0x00, 0x00, 0x00];
        assert_eq!(buff.read_header().unwrap(), MsgHeader::TerminationMark);
    }

    #[test]
    fn should_read_tm_body() {
        let mut buff: &[u8] = &[ 0x01, 0x02, 0x03, 0x04 ];
        let mut data = Vec::new();
        assert_eq!(buff.read_body(&MsgHeader::TerminationMark, &mut data).unwrap(), 0);
        assert_eq!(data.len(), 0);
    }

    #[test]
    fn should_fail_to_read_from_invalid_stream() {
        let mut buff: &[u8] = &[0x01, 0x02, 0x03, 0x04];
        let expected_error = ErrorKind::InvalidData;
        let actual_error = buff.read_header().err().unwrap().kind();
        assert_eq!(actual_error, expected_error);
    }

    #[test]
    fn should_write_rsd_header() {
        let mut buff = Cursor::new(Vec::new());
        let msg = MsgHeader::ReadStateData {
            offset: 0x1000,
            len: 4,
            target: 0x2000 as *mut u8,
        };
        assert_eq!(buff.write_header(&msg).unwrap(), 16);
        buff.set_position(0);
        assert_eq!(buff.get_ref().len(), 16);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 1);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 0x1000);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 4);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 0x2000);
    }

    #[test]
    fn should_write_rsd_body() {
        let mut buff = Vec::new();
        let mut input = Cursor::new(vec![ 0x01u8, 0x02, 0x03, 0x04 ]);
        let msg = MsgHeader::ReadStateData {
            offset: 0x1000,
            len: 4,
            target: 0x2000 as *mut u8,
        };

        assert_eq!(buff.write_body(&msg, &mut input).unwrap(), 4);
        assert_eq!(buff[0], 1);
        assert_eq!(buff[1], 2);
        assert_eq!(buff[2], 3);
        assert_eq!(buff[3], 4);
    }

    #[test]
    fn should_write_rsd() {
        let mut buff = Cursor::new(Vec::new());
        assert_eq!(buff.write_rsd(0x1000, 0x2000 as *mut u8, 4).unwrap(), 20);
        buff.set_position(0);
        assert_eq!(buff.get_ref().len(), 20);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 1);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 0x1000);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 4);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 0x2000);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 0);
    }

    #[test]
    fn should_write_wsd_header() {
        let mut buff = Cursor::new(Vec::new());
        let msg = MsgHeader::WriteStateData {
            offset: 0x1000,
            len: 4,
        };
        assert_eq!(buff.write_header(&msg).unwrap(), 12);
        buff.set_position(0);
        assert_eq!(buff.get_ref().len(), 12);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 2);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 0x1000);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 4);
    }

    #[test]
    fn should_write_wsd_body() {
        let mut buff = Vec::new();
        let mut input = Cursor::new(vec![ 0x01u8, 0x02, 0x03, 0x04 ]);
        let msg = MsgHeader::WriteStateData {
            offset: 0x1000,
            len: 4,
        };

        assert_eq!(buff.write_body(&msg, &mut input).unwrap(), 4);
        assert_eq!(buff[0], 1);
        assert_eq!(buff[1], 2);
        assert_eq!(buff[2], 3);
        assert_eq!(buff[3], 4);
    }

    #[test]
    fn should_write_wsd() {
        let mut buff = Cursor::new(Vec::new());
        let data = &0x01020304u32 as *const u32 as *const u8;
        assert_eq!(buff.write_wsd(0x1000, data, 4).unwrap(), 16);
        buff.set_position(0);
        assert_eq!(buff.get_ref().len(), 16);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 2);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 0x1000);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 4);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 0x01020304);
    }

    #[test]
    fn should_write_tm_header() {
        let mut buff = Cursor::new(Vec::new());
        assert_eq!(buff.write_header(&MsgHeader::TerminationMark).unwrap(),4);
        buff.set_position(0);
        assert_eq!(buff.read_u32::<LittleEndian>().unwrap(), 0);
    }

    #[test]
    fn should_write_tm_body() {
        let mut buff = Vec::new();
        let mut input = Cursor::new(vec![ 0x01u8, 0x02, 0x03, 0x04 ]);
        assert_eq!(buff.write_body(&MsgHeader::TerminationMark, &mut input).unwrap(), 0);
        assert_eq!(buff.len(), 0);
    }
}
