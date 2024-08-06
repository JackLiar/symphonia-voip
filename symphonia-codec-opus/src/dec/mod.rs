use std::io::Write;

use crate::errors::{Error, Result};
use crate::{Channels, SampleRate};

mod packet;
use packet::{get_nb_samples, parse_opus_pkt};

#[derive(Clone, Copy, Debug, Default)]
pub struct OpusDecoder {
    celt_dec_offset: i32,
    silk_dec_offset: i32,
    channels: Channels,
    /// Sampling rate (at the API level)
    fs: SampleRate,
    // silk_ctl DecControl;
    decode_gain: i32,
    complexity: i32,
    arch: i32,
    // #[cfg(feature = "deep-plc")]
    //  lpcnet: LPCNetPLCState,
    /// Everything beyond this point gets cleared on a reset
    //  #define OPUS_DECODER_RESET_START stream_channels
    stream_channels: Channels,
    bandwidth: i32,
    mode: i32,
    prev_mode: i32,
    frame_size: usize,
    prev_redundancy: i32,
    last_packet_duration: u64,
    #[cfg(feature = "fixed-point")]
    softclip_mem: [i16; 2],
    range_final: u32,
}

impl OpusDecoder {
    pub fn validate(&self) -> Result<()> {
        if self.stream_channels != self.channels {
            return Err(Error::InvalidDecoderParam(format!(
                "channels{} is not equals to stream_channels{}",
                self.channels as u8, self.stream_channels as u8
            )));
        }
        Ok(())
    }

    pub fn get_size() -> usize {
        unimplemented!()
    }

    pub fn new(fs: SampleRate, channels: Channels) -> Self {
        Self {
            fs,
            channels,
            stream_channels: channels,
            frame_size: (fs as usize) / 400,
            ..Default::default()
        }
    }

    pub fn decode<W: Write>(&mut self, data: &[u8], pcm: &mut W, decode_fec: bool) -> Result<()> {
        let nos = get_nb_samples(data, self.fs).map_err(|e| Error::InvalidPacket(e.to_string()))?;
        Ok(())
    }

    pub fn decode_native<W: Write>(
        &mut self,
        mut data: &[u8],
        pcm: &mut W,
        decode_fec: bool,
        nos: usize,
        self_delimited: bool,
        soft_clip: bool,
    ) -> Result<()> {
        if (data.is_empty() || decode_fec) && (nos % (self.fs as usize / 400) != 0) {
            let mut pcm_cnt = 0usize;
            while pcm_cnt < nos {
                let cnt = self.decode_frame(data)?;
                pcm_cnt += cnt;
            }
            debug_assert_eq!(pcm_cnt, nos);
            self.last_packet_duration += pcm_cnt as u64;
        }

        let (toc, frames) = parse_opus_pkt(data, self_delimited).map_err(|e| Error::InvalidPacket(e.to_string()))?;

        Ok(())
    }

    fn decode_frame(&mut self, data: &[u8]) -> Result<usize> {
        Ok(0)
    }
}
