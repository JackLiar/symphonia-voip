use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use binrw::{BinRead, BinResult};
use symphonia_core::errors::Result;
use symphonia_core::io::{MediaSourceStream, ReadBytes};

pub const MAGIC: &[u8] = b"#!rtpplay1.0 ";

#[allow(unused_variables)]
#[binrw::parser(reader, endian)]
fn parse_src_ip() -> BinResult<IpAddr> {
    let pos = reader.stream_position()?;
    let mut ip = vec![];

    loop {
        let char = &mut [0];
        reader.read_exact(char)?;
        if char[0] == b'/' {
            break;
        }
        ip.push(char[0]);
    }

    if ip.contains(&b'.') {
        let ip = Ipv4Addr::from_str(&String::from_utf8_lossy(&ip))
            .map_err(|e| binrw::Error::Custom { pos, err: Box::new(e) })?;
        Ok(IpAddr::V4(ip))
    } else {
        let ip = Ipv6Addr::from_str(&String::from_utf8_lossy(&ip))
            .map_err(|e| binrw::Error::Custom { pos, err: Box::new(e) })?;
        Ok(IpAddr::V6(ip))
    }
}

#[allow(unused_variables)]
#[binrw::parser(reader, endian)]
fn parse_src_port() -> BinResult<u16> {
    let pos = reader.stream_position()?;
    let port: &mut [u8] = &mut [0; 6];
    let mut len = 0;

    for c in port.iter_mut() {
        let char = &mut [0];
        reader.read_exact(char)?;
        if char[0] == b'\n' {
            break;
        }
        *c = char[0];
        len += 1;
    }

    String::from_utf8_lossy(&port[..len])
        .parse::<u16>()
        .map_err(|e| binrw::Error::Custom { pos, err: Box::new(e) })
}

#[derive(BinRead, Clone, Copy, Debug)]
#[br(big, magic = b"#!rtpplay1.0 ")]
#[repr(C)]
pub struct FileHeader {
    #[br(parse_with = parse_src_ip)]
    pub ip: IpAddr,
    #[br(parse_with = parse_src_port)]
    pub port: u16,
    pub start_sec: u32,
    pub start_usec: u32,
    pub ip2: u32,
    pub port2: u16,
    pub padding: u16,
}

impl Default for FileHeader {
    fn default() -> Self {
        Self {
            ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            port: 0,
            start_sec: 0,
            start_usec: 0,
            ip2: 0,
            port2: 0,
            padding: 0,
        }
    }
}

#[derive(BinRead, Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct RDPacket {
    /// length of packet, including this header (may be smaller than plen if not whole packet recorded)
    pub len: u16,
    /// actual header+payload length for RTP, 0 for RTCP
    pub org_len: u16,
    /// milliseconds since the start of recording
    pub offset: u32,
}

pub fn read_rd_pkt(source: &mut MediaSourceStream) -> Result<Box<[u8]>> {
    let len = source.read_be_u16()?;
    let org_len = source.read_be_u16()?;
    let offset = source.read_be_u32()?;
    let pkt = RDPacket { len, org_len, offset };
    Ok(source.read_boxed_slice_exact(pkt.org_len as usize)?)
}
