use symphonia_core::codecs::{
    CodecParameters, CodecType, CODEC_TYPE_PCM_ALAW, CODEC_TYPE_PCM_MULAW,
};
use symphonia_core::errors::{unsupported_error, Result};

use codec_detector::{rtp::RtpPacket, Codec};
use symphonia_bundle_amr::rtp::{on_amr_amrwb_be, on_amr_amrwb_oa};
use symphonia_bundle_amr::{DecoderParams as AMRDecodeParams, CODEC_TYPE_AMR, CODEC_TYPE_AMRWB};
use symphonia_bundle_evs::dec::CODEC_TYPE_EVS;
use symphonia_codec_g7221::CODEC_TYPE_G722_1;

use crate::utils::bytes_to_struct;

pub fn codec_to_codec_type(codec: &Codec) -> Option<CodecType> {
    let ct = match codec.name.to_lowercase().as_str() {
        "amr" => CODEC_TYPE_AMR,
        "amrwb" => CODEC_TYPE_AMRWB,
        "evs" => CODEC_TYPE_EVS,
        "g.722.1" => CODEC_TYPE_G722_1,
        "pcma" => CODEC_TYPE_PCM_ALAW,
        "pcmu" => CODEC_TYPE_PCM_MULAW,
        _ => return None,
    };
    Some(ct)
}

pub fn parse_rtp_payload<R: RtpPacket>(params: &CodecParameters, rtp: &R) -> Result<Vec<u8>> {
    match params.codec {
        CODEC_TYPE_G722_1 | CODEC_TYPE_PCM_ALAW | CODEC_TYPE_PCM_MULAW => {
            return Ok(rtp.payload().to_vec())
        }
        CODEC_TYPE_AMR | CODEC_TYPE_AMRWB => {
            let param: AMRDecodeParams = params
                .extra_data
                .as_ref()
                .map(|d| bytes_to_struct(d))
                .unwrap_or_default();
            let mut pkt = vec![];
            if param.octet_align {
                on_amr_amrwb_oa(&mut pkt, rtp.payload(), params.codec)?;
                Ok(pkt)
            } else {
                on_amr_amrwb_be(&mut pkt, rtp.payload(), params.codec)?;
                Ok(pkt)
            }
        }
        CODEC_TYPE_EVS => {
            let mut pkt = vec![];
            symphonia_bundle_evs::rtp::on_evs(&mut pkt, rtp.payload())?;
            Ok(pkt)
        }
        _ => return unsupported_error("Unsupport codec"),
    }
}
