use std::io::{Seek, SeekFrom};

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

use crate::dec::CODEC_TYPE_AMRWB;
use crate::{AMRWB_BUFFER_SIZE, AMRWB_SAMPLE_RATE};

const AMRWB_MIME_MAGIC: &[u8] = b"#!AMR-WB\n";
const AMRWB_MC_MIME_MAGIC: &[u8] = b"#!AMR-WB_MC1.0\n";

/// See RFC 4867 section 5.3
#[derive(Clone, Copy, Debug)]
struct AmrwbToc(pub u8);

impl AmrwbToc {
    const AMRWB_PAYLOAD_SIZES: &'static [isize] =
        &[17, 23, 32, 36, 40, 46, 50, 58, 60, 5, -1, -1, -1, -1, -1, 0];

    /// Frame quality indicator
    pub fn q(&self) -> bool {
        ((self.0 >> 2) & 0x01) == 1
    }

    /// Frame type index
    pub fn ft(&self) -> usize {
        ((self.0 >> 3) & 0x0f) as usize
    }

    pub fn payload_size(&self) -> Option<usize> {
        match Self::AMRWB_PAYLOAD_SIZES.get(self.ft()) {
            None => None,
            Some(s) if *s < 0 => None,
            Some(s) => Some(*s as usize),
        }
    }
}

/// AMRWB format reader.
///
/// `AmrwbReader` implements a demuxer for the MIME AMRWB container format.
pub struct AmrwbReader {
    reader: MediaSourceStream,
    tracks: Vec<Track>,
    cues: Vec<Cue>,
    metadata: MetadataLog,
    consumed: usize,
    channels: usize,
    chl_idx: usize,
    track_ts: Vec<u64>,
}

impl AmrwbReader {
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

impl QueryDescriptor for AmrwbReader {
    fn query() -> &'static [symphonia_core::probe::Descriptor] {
        &[support_format!(
            "amrwb",
            "Adaptive Multi-Rate Wideband Storage Format",
            &["amrwb"],
            &["audio/AMRWB"],
            &[AMRWB_MIME_MAGIC, AMRWB_MC_MIME_MAGIC]
        )]
    }

    fn score(context: &[u8]) -> u8 {
        255
    }
}

impl FormatReader for AmrwbReader {
    fn try_new(source: MediaSourceStream, _options: &FormatOptions) -> Result<Self> {
        let mut amr = Self::new(source);
        let consumed = AMRWB_MIME_MAGIC.len();

        let magic = amr.reader.read_boxed_slice_exact(AMRWB_MIME_MAGIC.len())?;
        if magic.as_ref() != AMRWB_MIME_MAGIC {
            return Err(Error::DecodeError("Invalid AMRWB MIME header"));
        }

        amr.channels = 1;

        for cid in 0..amr.channels {
            let mut codec_params = CodecParameters::new();
            codec_params.codec = CODEC_TYPE_AMRWB;
            codec_params.channels = Some(Channels::FRONT_CENTRE);
            codec_params
                .with_sample_rate(AMRWB_SAMPLE_RATE)
                .with_time_base(TimeBase::new(1, AMRWB_SAMPLE_RATE));

            amr.consumed = consumed;
            amr.tracks.push(Track::new(cid as u32, codec_params));
            amr.track_ts.push(0);
        }

        Ok(amr)
    }

    fn next_packet(&mut self) -> Result<Packet> {
        let mut data_len = 0;
        let toc = AmrwbToc(self.reader.read_byte()?);
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
            self.track_ts[self.chl_idx] * AMRWB_BUFFER_SIZE,
            AMRWB_BUFFER_SIZE,
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
