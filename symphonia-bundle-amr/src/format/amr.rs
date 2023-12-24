use std::io::{Seek, SeekFrom};

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

use crate::dec::CODEC_TYPE_AMR;
use crate::{AMR_BUFFER_SIZE, AMR_SAMPLE_RATE};

const AMR_MIME_MAGIC: &[u8] = b"#!AMR\n";
const AMR_MC_MIME_MAGIC: &[u8] = b"#!AMR_MC1.0\n";

/// See RFC 4867 section 5.3
#[derive(Clone, Copy, Debug)]
struct AmrToc(pub u8);

impl AmrToc {
    const AMR_PAYLOAD_SIZES: &'static [usize] =
        &[12, 13, 15, 17, 19, 20, 26, 31, 5, 6, 5, 5, 0, 0, 0, 0];

    /// Frame quality indicator
    pub fn q(&self) -> bool {
        ((self.0 >> 2) & 0x01) == 1
    }

    /// Frame type index
    pub fn ft(&self) -> usize {
        ((self.0 >> 3) & 0x0f) as usize
    }

    pub fn payload_size(&self) -> Option<usize> {
        Self::AMR_PAYLOAD_SIZES.get(self.ft()).map(|s| *s)
    }
}

/// AMR format reader.
///
/// `AmrReader` implements a demuxer for the MIME EVS container format.
pub struct AmrReader {
    reader: MediaSourceStream,
    tracks: Vec<Track>,
    cues: Vec<Cue>,
    metadata: MetadataLog,
    consumed: usize,
    channels: usize,
    chl_idx: usize,
    track_ts: Vec<u64>,
}

impl AmrReader {
    pub fn new(reader: MediaSourceStream) -> Self {
        Self {
            reader,
            tracks: Default::default(),
            cues: Default::default(),
            metadata: Default::default(),
            consumed: 0,
            channels: 0,
            chl_idx: 0,
            track_ts: vec![],
        }
    }
}

impl QueryDescriptor for AmrReader {
    fn query() -> &'static [symphonia_core::probe::Descriptor] {
        &[support_format!(
            "amr",
            "Adaptive Multi-Rate Storage Format",
            &["amr"],
            &["audio/AMR"],
            &[AMR_MIME_MAGIC, AMR_MC_MIME_MAGIC]
        )]
    }

    fn score(context: &[u8]) -> u8 {
        255
    }
}

impl FormatReader for AmrReader {
    fn try_new(source: MediaSourceStream, options: &FormatOptions) -> Result<Self> {
        let mut amr = Self::new(source);
        let consumed = AMR_MIME_MAGIC.len();

        let magic = amr.reader.read_boxed_slice_exact(AMR_MIME_MAGIC.len())?;
        if magic.as_ref() != AMR_MIME_MAGIC {
            return Err(Error::DecodeError("Invalid AMR MIME header"));
        }

        amr.channels = 1;

        for cid in 0..amr.channels {
            let mut codec_params = CodecParameters::new();
            codec_params.codec = CODEC_TYPE_AMR;
            codec_params.with_sample_rate(AMR_SAMPLE_RATE);
            codec_params
                .with_sample_rate(AMR_SAMPLE_RATE)
                .with_time_base(TimeBase::new(1, AMR_SAMPLE_RATE));

            amr.consumed = consumed;
            amr.tracks.push(Track::new(cid as u32, codec_params));
            amr.track_ts.push(0);
        }

        Ok(amr)
    }

    fn next_packet(&mut self) -> Result<Packet> {
        let mut data_len = 0;
        let toc = AmrToc(self.reader.read_byte()?);
        data_len += 1;
        self.consumed += 1;

        if let Some(len) = toc.payload_size() {
            data_len += len;
            self.consumed += len;
        };

        self.reader.seek(SeekFrom::Current(-1))?;
        let data = self.reader.read_boxed_slice_exact(data_len)?;

        let pkt = Packet::new_from_boxed_slice(
            self.chl_idx as u32,
            self.track_ts[self.chl_idx] * AMR_BUFFER_SIZE,
            AMR_BUFFER_SIZE,
            data,
        );
        self.track_ts[self.chl_idx] += 1;

        // update internal channel index
        self.chl_idx = (self.chl_idx) / self.channels;

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

    fn seek(&mut self, mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        if self.tracks.is_empty() {
            return seek_error(SeekErrorKind::Unseekable);
        }

        unimplemented!()
    }

    fn into_inner(self: Box<Self>) -> MediaSourceStream {
        self.reader
    }
}
