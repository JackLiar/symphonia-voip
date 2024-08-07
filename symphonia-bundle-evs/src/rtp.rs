use std::io::Write;

use byteorder::ReadBytesExt;

pub const EVS_PAYLOAD_SIZES_PRIMARY: &[usize] =
    &[6, 18, 20, 24, 33, 41, 61, 80, 120, 160, 240, 320];
pub const EVS_PAYLOAD_SIZES_PRIMARY_TOC: &[&[u8; 1]] = &[
    b"\x0C", b"\x01", b"\x02", b"\x03", b"\x04", b"\x05", b"\x06", b"\x07", b"\x08", b"\x09",
    b"\x0A", b"\x0B",
];
pub const EVS_PAYLOAD_SIZES_AMRWBIO: &[usize] = &[7, 17, 23, 32, 36, 40, 46, 50, 58, 60];
pub const EVS_PAYLOAD_SIZES_AMRWBIO_TOC: &[&[u8; 1]] = &[
    b"\x00", b"0", b"1", b"2", b"3", b"4", b"5", b"6", b"7", b"8",
];

#[derive(Clone, Copy, Debug, Default)]
pub enum FramingMode {
    #[default]
    Compat,
    HeaderFull,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum EVSMode {
    #[default]
    Primary = 0,
    AMRWBIO = 1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeaderType {
    ToC = 0,
    CMR = 1,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Toc(u8);

impl Toc {
    pub fn header_type(&self) -> HeaderType {
        match self.0 >> 7 {
            0 => HeaderType::ToC,
            1 => HeaderType::CMR,
            _ => unreachable!(),
        }
    }

    pub fn followed_by_another_speech_frame(&self) -> bool {
        ((self.0 >> 6) & 1) == 1
    }

    pub fn evs_mode(&self) -> EVSMode {
        match (self.0 >> 5) & 1 {
            0 => EVSMode::Primary,
            1 => EVSMode::AMRWBIO,
            _ => unreachable!(),
        }
    }

    pub fn bit_rate_idx(&self) -> usize {
        (self.0 & 0x0f) as usize
    }
}

pub fn parse_evs(mut data: &[u8]) -> std::io::Result<(Vec<&[u8]>, &[u8])> {
    let frm_mode = if EVS_PAYLOAD_SIZES_PRIMARY.contains(&data.len()) {
        FramingMode::Compat
    } else {
        FramingMode::HeaderFull
    };

    let mut frm_sizes = vec![];
    match frm_mode {
        FramingMode::Compat => {
            frm_sizes.push(data.len());
        }
        FramingMode::HeaderFull => {
            let mut tmp = data;
            let toc = Toc(tmp.read_u8()?);
            if toc.header_type() == HeaderType::CMR {
                data = &data[1..];
            }

            loop {
                let toc = Toc(data.read_u8()?);
                let size = match toc.evs_mode() {
                    EVSMode::Primary => {
                        if toc.bit_rate_idx() > EVS_PAYLOAD_SIZES_PRIMARY.len() - 1 {
                            1
                        } else {
                            EVS_PAYLOAD_SIZES_PRIMARY[toc.bit_rate_idx()]
                        }
                    }
                    EVSMode::AMRWBIO => {
                        if toc.bit_rate_idx() > EVS_PAYLOAD_SIZES_AMRWBIO.len() - 1 {
                            1
                        } else {
                            EVS_PAYLOAD_SIZES_AMRWBIO[toc.bit_rate_idx()]
                        }
                    }
                };
                frm_sizes.push(size);
                if !toc.followed_by_another_speech_frame() {
                    break;
                }
            }
        }
    }

    let mut frames = vec![];
    if data != [0] && !data.is_empty() && data.len() != frm_sizes.len() {
        for size in frm_sizes {
            if data.len() >= size {
                frames.push(&data[..size]);
                data = &data[size..];
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!("Expecting {} bytes, {} remain", size, data.len()),
                ));
            }
        }
    } else {
        data = &[];
    }

    Ok((frames, data))
}

pub fn on_evs(r: &mut dyn Write, data: &[u8]) -> std::io::Result<()> {
    let (frames, _) = parse_evs(data)?;

    for frm in frames {
        if let Some(idx) = EVS_PAYLOAD_SIZES_PRIMARY
            .iter()
            .position(|s| *s == frm.len())
        {
            let toc = EVS_PAYLOAD_SIZES_PRIMARY_TOC[idx];
            r.write_all(toc)?;
            r.write_all(frm)?;
        }
    }

    Ok(())
}

pub fn is_evs(data: &[u8]) -> bool {
    parse_evs(data).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_evs_compat_header() -> std::io::Result<()> {
        let data: &[u8] = &[
            0x62, 0x8e, 0xc0, 0x6e, 0x60, 0xee, 0x6a, 0x10, 0x74, 0x6d, 0xf7, 0xc3, 0x50, 0xb2,
            0xf1, 0x80, 0xcf, 0x0f, 0xda, 0xe6, 0x71, 0xc8, 0x10, 0xe7, 0x6e, 0x71, 0x97, 0x30,
            0x5b, 0x58, 0x74, 0xc4, 0x0c,
        ];

        let (frames, rem) = parse_evs(data)?;
        assert_eq!(frames.len(), 1);
        assert!(rem.is_empty());

        Ok(())
    }

    #[test]
    fn test_parse_evs_full_header() -> std::io::Result<()> {
        let data: &[u8] = &[
            0x45, 0x45, 0x45, 0x45, 0x45, 0x45, 0x45, 0x45, 0x45, 0x45, 0x45, 0x05, 0x81, 0x4d,
            0x6e, 0x4e, 0x68, 0x40, 0x74, 0x02, 0x02, 0x02, 0x02, 0x02, 0xa8, 0x11, 0xf2, 0x11,
            0x76, 0xa2, 0x8c, 0x05, 0xe1, 0x39, 0xc4, 0xf7, 0xe3, 0x7f, 0x3e, 0x0b, 0xb2, 0x9a,
            0x7e, 0xb3, 0x97, 0xdc, 0xd7, 0xfd, 0xd0, 0xcb, 0x1b, 0x50, 0xa4, 0x90, 0x63, 0x10,
            0x1e, 0x78, 0x4c, 0xcf, 0x71, 0xdc, 0x34, 0xc4, 0x21, 0xb3, 0xb1, 0xe5, 0xca, 0xb2,
            0x4f, 0xaa, 0xdc, 0x2a, 0x47, 0x73, 0xd6, 0x31, 0x8f, 0x7f, 0xd2, 0x4f, 0x34, 0x75,
            0x44, 0x6d, 0x42, 0x9a, 0x4d, 0xed, 0x6c, 0x3f, 0xa7, 0x12, 0x91, 0x4c, 0x9a, 0xad,
            0x95, 0x40, 0xc7, 0xa5, 0xfc, 0x3a, 0x50, 0x06, 0x09, 0xb2, 0x76, 0x77, 0xa0, 0xd9,
            0xf8, 0x78, 0x11, 0x29, 0x0d, 0xf1, 0x7e, 0x7f, 0x21, 0xb1, 0x19, 0x17, 0x63, 0x80,
            0x4f, 0x5d, 0xd8, 0x1a, 0xdb, 0x6d, 0x34, 0x7d, 0xfa, 0x91, 0x60, 0x6a, 0x97, 0x4d,
            0x3d, 0x07, 0xa5, 0xde, 0xba, 0x4e, 0x1e, 0x5c, 0xaf, 0xfb, 0x7b, 0x63, 0x9e, 0x25,
            0x56, 0xdc, 0x19, 0x91, 0x36, 0x30, 0x42, 0xe9, 0x82, 0xe4, 0xb4, 0x7f, 0xc7, 0x94,
            0xa6, 0xdd, 0x6b, 0xe7, 0xb0, 0x3d, 0xdf, 0x40, 0x91, 0x61, 0x52, 0xee, 0xa9, 0x3e,
            0xc7, 0xa5, 0xdd, 0xbd, 0x24, 0x80, 0xfb, 0xbb, 0xb2, 0xda, 0x1d, 0x44, 0xa3, 0x6a,
            0x74, 0x5c, 0x2b, 0xc7, 0x11, 0x3c, 0x06, 0x60, 0x26, 0x62, 0x0d, 0x19, 0x3b, 0x33,
            0xae, 0x90, 0x3d, 0x60, 0xbd, 0x2b, 0x54, 0x91, 0x4e, 0x1f, 0xbb, 0xb6, 0x38, 0xc7,
            0x98, 0x2b, 0xcb, 0xa4, 0x80, 0x55, 0xde, 0xd2, 0xe8, 0xef, 0x7e, 0x2e, 0xfc, 0x8c,
            0x37, 0x40, 0x99, 0x66, 0xa6, 0xab, 0x21, 0xf5, 0xea, 0xb0, 0xa8, 0x2e, 0x91, 0x42,
            0x51, 0x0b, 0xc1, 0x6e, 0xfb, 0x58, 0x91, 0x4d, 0xa8, 0x34, 0xbb, 0x37, 0x07, 0xb8,
            0xb5, 0x3a, 0x4f, 0x0c, 0x59, 0x2e, 0xe1, 0xb5, 0xf5, 0xe2, 0xff, 0x29, 0x19, 0xb3,
            0xda, 0x1c, 0x20, 0xa0, 0xcd, 0xb1, 0xcf, 0xed, 0xab, 0xae, 0x3d, 0x76, 0xe5, 0xdd,
            0xbc, 0x5d, 0x4f, 0x97, 0x71, 0x91, 0x64, 0x23, 0x3e, 0x3c, 0x3c, 0xc7, 0xa5, 0xdd,
            0xed, 0x23, 0x05, 0x98, 0x02, 0x93, 0x8e, 0xdf, 0x11, 0x77, 0xbd, 0x87, 0xcd, 0x37,
            0xf9, 0xf5, 0xe4, 0xd2, 0xd4, 0x73, 0x42, 0x7d, 0x82, 0xc0, 0x8a, 0x16, 0xe9, 0xb6,
            0xb3, 0x6c, 0xb9, 0x46, 0x91, 0x51, 0xba, 0x17, 0x29, 0x3c, 0xc7, 0xa5, 0xf8, 0x7e,
            0x90, 0xc3, 0x35, 0xd1, 0x37, 0xf7, 0x77, 0x86, 0x84, 0xac, 0xb6, 0x90, 0xa6, 0xdb,
            0x72, 0x6b, 0x1d, 0x55, 0xba, 0x4a, 0x1a, 0x10, 0x72, 0x88, 0x2d, 0xb5, 0xc0, 0x52,
            0xb0, 0x36, 0xac, 0x91, 0x71, 0x33, 0x89, 0xb0, 0x43, 0x07, 0xb4, 0x5e, 0xf5, 0xa4,
            0x83, 0x86, 0x30, 0x7f, 0xde, 0xeb, 0x5e, 0x42, 0xb3, 0x0e, 0x41, 0x1a, 0x98, 0xc8,
            0x60, 0x3c, 0xdb, 0xaa, 0xb9, 0xb4, 0x7b, 0xa5, 0x54, 0x8a, 0x9d, 0x5a, 0xdb, 0x5c,
            0x52, 0x50, 0x91, 0x4f, 0x59, 0x76, 0x7e, 0x42, 0xc7, 0xb6, 0x21, 0x7d, 0x25, 0x88,
            0x80, 0xd0, 0x81, 0x30, 0xde, 0xbb, 0x94, 0xc2, 0x1b, 0x45, 0x7b, 0xa2, 0xd6, 0x96,
            0x26, 0x44, 0x96, 0xf6, 0xeb, 0x9c, 0xcf, 0x7d, 0x2a, 0xa2, 0xaf, 0xe7, 0xe8, 0x83,
            0xe0, 0x91, 0x60, 0xad, 0xf6, 0xae, 0x3a, 0xc7, 0xb6, 0x3e, 0x9d, 0x21, 0x83, 0x6f,
            0x9f, 0x35, 0x1f, 0x2b, 0x4e, 0xb9, 0x86, 0xd3, 0x24, 0xf1, 0x29, 0xd7, 0x32, 0x12,
            0xcf, 0x11, 0x4e, 0xb9, 0x6a, 0x73, 0x55, 0x1c, 0x2b, 0x56, 0x6b, 0x50, 0xaf, 0xa2,
        ];

        let (frames, rem) = parse_evs(data)?;
        assert_eq!(frames.len(), 12);
        assert!(rem.is_empty());

        let data: &[u8] = &[
            0x04, 0xf7, 0xe4, 0x96, 0x4f, 0xde, 0x0f, 0xa1, 0xc0, 0x1b, 0xbb, 0x28, 0xd0, 0xd4,
            0x83, 0xf1, 0xab, 0x60, 0x8a, 0xf1, 0xb4, 0x0b, 0xfc, 0x21, 0xd8, 0x63, 0x5d, 0x2f,
            0xf9, 0xe0, 0x1c, 0xe7, 0xb8, 0x48,
        ];
        let (frames, rem) = parse_evs(data)?;
        assert_eq!(frames.len(), 1);
        assert!(rem.is_empty());

        let data: &[u8] = &[0xf4, 0xf3, 0xe7, 0xcf, 0x7c, 0x98, 0x00];
        let (frames, rem) = parse_evs(data)?;
        assert_eq!(frames.len(), 0);
        assert!(rem.is_empty());

        let data: &[u8] = &[0xf4, 0xf5, 0xfb, 0xcf, 0x78, 0x78, 0x00];
        let (frames, rem) = parse_evs(data)?;
        assert_eq!(frames.len(), 0);
        assert!(rem.is_empty());

        let data: &[u8] = &[0xf4, 0xfa, 0xf7, 0xbf, 0x6c, 0xa8, 0x00];
        let (frames, rem) = parse_evs(data)?;
        assert_eq!(frames.len(), 0);
        assert!(rem.is_empty());

        Ok(())
    }
}
