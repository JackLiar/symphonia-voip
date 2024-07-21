use std::io::{Seek, SeekFrom};
use std::num::NonZeroUsize;

use symphonia_core::audio::Channels;
use symphonia_core::codecs::CodecParameters;
use symphonia_core::errors::{seek_error, Error, Result, SeekErrorKind};
use symphonia_core::formats::{
    Cue, FormatOptions, FormatReader, Packet, SeekMode, SeekTo, SeekedTo, Track,
};
use symphonia_core::io::{MediaSourceStream, ReadBytes};
use symphonia_core::meta::{Metadata, MetadataLog};
use symphonia_core::probe::{Descriptor, Instantiate, QueryDescriptor};
use symphonia_core::support_format;
use symphonia_core::units::TimeBase;

use crate::dec::DecoderParams;
use crate::EvsToc;

const EVS_MIME_MAGIC: &[u8] = b"#!EVS_MC1.0\n";

pub struct EvsReaderBuilder(EvsReader);

impl EvsReaderBuilder {
    /// Set track amount
    pub fn with_tracks(mut self, cnt: usize) -> Self {
        self.0.tracks = vec![];
        self
    }

    /// Set sample rate
    pub fn with_sample_rate(mut self, sample_rate: u32) -> Self {
        self.0.sample_rate = Some(sample_rate);
        self
    }

    /// Set timestamp interval
    pub fn with_timestamp_interval(mut self, intv: u64) -> Self {
        self.0.timestamp_interval = intv;
        self
    }
}

/// EVS format reader.
///
/// `EvsReader` implements a demuxer for the MIME EVS container format.
pub struct EvsReader {
    reader: MediaSourceStream,
    tracks: Vec<Track>,
    track_ts: Vec<u64>,
    cues: Vec<Cue>,
    metadata: MetadataLog,
    channels: usize,
    chl_idx: usize,
    pkt_cnt: u64,
    pub sample_rate: Option<u32>,
    pub timestamp_interval: u64,
}

impl EvsReader {
    pub fn new(reader: MediaSourceStream) -> Self {
        Self {
            reader,
            tracks: Default::default(),
            track_ts: Default::default(),
            cues: Default::default(),
            metadata: Default::default(),
            channels: 0,
            chl_idx: 0,
            pkt_cnt: 0,
            sample_rate: Some(16000),
            timestamp_interval: 320,
        }
    }
}

impl QueryDescriptor for EvsReader {
    fn query() -> &'static [symphonia_core::probe::Descriptor] {
        &[support_format!(
            "evs",
            "Enhanced Voice Service Storage Format",
            &["evs"],
            &["audio/EVS"],
            &[EVS_MIME_MAGIC]
        )]
    }

    fn score(_context: &[u8]) -> u8 {
        255
    }
}

impl FormatReader for EvsReader {
    fn try_new(source: MediaSourceStream, _options: &FormatOptions) -> Result<Self> {
        let mut evs = Self::new(source);
        let mut consumed = 0;

        let magic = evs.reader.read_boxed_slice_exact(EVS_MIME_MAGIC.len())?;
        if magic.as_ref() != EVS_MIME_MAGIC {
            return Err(Error::DecodeError("Invalid EVS MIME header"));
        }
        consumed += EVS_MIME_MAGIC.len();

        evs.channels = evs.reader.read_be_u32()? as usize;
        consumed += 4;

        for cid in 0..evs.channels {
            let mut codec_params = CodecParameters::new();
            codec_params.codec = crate::dec::CODEC_TYPE_EVS;
            codec_params.channels = Some(Channels::FRONT_CENTRE);
            if let Some(sr) = evs.sample_rate {
                codec_params
                    .with_sample_rate(sr)
                    .with_time_base(TimeBase::new(1, sr));
            }

            let param = Box::new(DecoderParams {
                channel: NonZeroUsize::new(evs.channels)
                    .ok_or_else(|| Error::DecodeError("No channel found in file"))?,
                ..Default::default()
            });
            let param = unsafe { crate::utils::any_as_u8_slice(param.as_ref()) };
            let mut extra_data = Box::new([0; std::mem::size_of::<DecoderParams>()]);
            extra_data.copy_from_slice(param);
            codec_params.extra_data = Some(extra_data);
            evs.tracks.push(Track::new(cid as u32, codec_params));
            evs.track_ts.push(0);
        }

        Ok(evs)
    }

    fn next_packet(&mut self) -> Result<Packet> {
        // read toc byte
        let mut data_len = 0;
        let toc = EvsToc(self.reader.read_byte()?);
        data_len += 1;

        // if is a valid frame, read speech data
        if let Some(len) = toc.payload_size() {
            data_len += len;
        }

        // rewind position, because codec needs toc to get quality/bitrate information
        self.reader.seek(SeekFrom::Current(-1))?;

        // read all data
        let data = self.reader.read_boxed_slice_exact(data_len)?;

        let pkt = Packet::new_from_boxed_slice(
            self.chl_idx as u32,
            self.track_ts[self.chl_idx] * self.timestamp_interval,
            self.timestamp_interval,
            data,
        );
        self.track_ts[self.chl_idx] += 1;

        // update internal channel index
        self.chl_idx = (self.chl_idx + 1) % self.channels;

        Ok(pkt)
    }

    fn metadata(&mut self) -> Metadata<'_> {
        self.metadata.metadata()
    }

    fn cues(&self) -> &[Cue] {
        &self.cues
    }

    fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    fn seek(&mut self, _mode: SeekMode, _to: SeekTo) -> Result<SeekedTo> {
        if self.tracks.is_empty() {
            return seek_error(SeekErrorKind::Unseekable);
        }

        unimplemented!()
    }

    fn into_inner(self: Box<Self>) -> MediaSourceStream {
        self.reader
    }
}
