use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Error as IoError, ErrorKind, Write};
use std::path::PathBuf;

use bytemuck::cast_slice;
use clap::ArgMatches;
use log::warn;
use symphonia::core::audio::{
    AsAudioBufferRef, AudioBuffer, Channels, SampleBuffer, Signal, SignalSpec,
};
use symphonia::core::codecs::{CodecRegistry, DecoderOptions};
use symphonia::core::errors::{Error, Result};
use symphonia::core::formats::FormatReader;

use crate::{do_verification, ignore_end_of_stream_error};

#[allow(non_camel_case_types, clippy::upper_case_acronyms, dead_code)]
#[repr(u16)]
#[derive(Clone, Copy, Debug, Default)]
pub enum CompressionCode {
    Unknown = 0x0000,
    #[default]
    PCM = 0x0001,
    ADPCM = 0x0002,
}

pub fn decode_only_output(
    args: &ArgMatches,
    registry: &CodecRegistry,
    mut reader: Box<dyn FormatReader>,
    decode_opts: &DecoderOptions,
) -> Result<i32> {
    let output_dir = args.get_one::<PathBuf>("output-dir").unwrap();
    std::fs::create_dir_all(output_dir)?;
    let mut decoders = HashMap::new();
    let mut pcms = HashMap::new();
    for track in reader.tracks() {
        let decoder = registry.make(&track.codec_params, decode_opts)?;
        decoders.insert(track.id, decoder);

        let fname = format!("{:#010x}.wav", track.id);
        let fpath = output_dir.join(&fname);
        let buf = BufWriter::new(vec![]);
        let sr = track.codec_params.sample_rate.unwrap_or(16000);
        pcms.insert(track.id, (buf, fpath, sr));
    }

    // Decode all packets, ignoring all decode errors.
    let result = loop {
        let packet = match reader.next_packet() {
            Ok(packet) => packet,
            Err(err) => break Err(err),
        };

        let track = reader
            .tracks()
            .iter()
            .find(|t| t.id == packet.track_id())
            .unwrap();
        let sr = track.codec_params.sample_rate.unwrap() as u64;
        let decoder = decoders.get_mut(&track.id).unwrap();
        let (pcm, _, _) = pcms.get_mut(&track.id).unwrap();

        let mut buf =
            AudioBuffer::<u8>::new(sr / 50, SignalSpec::new(sr as u32, Channels::FRONT_CENTRE));
        let decoded = if packet.buf().is_empty() {
            // handle dummy rtp packet
            buf.render_silence(Some(sr as usize / 50));
            Ok(buf.as_audio_buffer_ref())
        } else {
            decoder.decode(&packet)
        };

        // Decode the packet into audio samples.
        match decoded {
            Ok(decoded) => {
                let duration = decoded.capacity() as u64;
                let spec = *decoded.spec();
                let mut samples = SampleBuffer::<i16>::new(duration, spec);
                samples.copy_interleaved_ref(decoded);
                pcm.write_all(cast_slice::<_, u8>(samples.samples()))?;
            }
            Err(Error::DecodeError(err)) => warn!("decode error: {}", err),
            Err(err) => break Err(err),
        }
    };

    // Return if a fatal error occured.
    ignore_end_of_stream_error(result)?;

    for (_, (pcm, fpath, sr)) in pcms {
        let pcm = pcm.into_inner().unwrap();
        let mut file = BufWriter::new(File::create(&fpath).map_err(|e| {
            IoError::new(
                ErrorKind::NotFound,
                format!("Failed to create {}, {}", fpath.display(), e),
            )
        })?);

        let sft = b"Symphonia voip-replay\x00";
        let riff_len = 4 + 8 + 16 + 8 + 4 + 8 + sft.len() as u32 + 8 + pcm.len() as u32;

        // write file header
        file.write_all(b"RIFF")?;
        file.write_all(&(riff_len).to_le_bytes())?;

        file.write_all(b"WAVE")?;

        // write format chunk
        file.write_all(b"fmt ")?;
        file.write_all(&16u32.to_le_bytes())?;

        // compression mode
        file.write_all(&(CompressionCode::PCM as u16).to_le_bytes())?;
        // number of channels, mono
        file.write_all(&1u16.to_le_bytes())?;
        // sample rate
        file.write_all(&sr.to_le_bytes())?;
        // avg bytes per seconds
        file.write_all(&(sr * 2 * 1).to_le_bytes())?;
        // block align
        file.write_all(&(2u16 * 1).to_le_bytes())?;
        // significant bits per sample
        file.write_all(&16u16.to_le_bytes())?;

        // write list chunk
        file.write_all(b"LIST")?;
        file.write_all(&34u32.to_le_bytes())?;

        file.write_all(b"INFO")?;
        file.write_all(b"ISFT")?;
        file.write_all(&(sft.len() as u32 + 1).to_le_bytes())?;
        file.write_all(sft)?;

        file.write_all(b"data")?;
        file.write_all(&(pcm.len() as u32).to_le_bytes())?;
        file.write_all(&pcm)?;
    }

    // Finalize the decoder and return the verification result if it's been enabled.
    for (_id, mut decoder) in decoders {
        do_verification(decoder.finalize())?;
    }
    Ok(0)
}
