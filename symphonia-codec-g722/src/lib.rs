use std::io::Write;

use bitflags::bitflags;
use bytemuck::cast_slice_mut;
use symphonia_core::audio::{
    AsAudioBufferRef, AudioBuffer, AudioBufferRef, Channels, Signal, SignalSpec,
};
use symphonia_core::codecs::{
    decl_codec_type, CodecDescriptor, CodecParameters, CodecType, Decoder as D, DecoderOptions,
    FinalizeResult,
};
use symphonia_core::errors::Result;
use symphonia_core::formats::Packet;
use symphonia_core::support_codec;

pub const CODEC_TYPE_G722: CodecType = decl_codec_type(b"g722");

const WL: [i32; 8] = [-60, -30, 58, 172, 334, 538, 1198, 3042];
const RL42: [i32; 16] = [0, 7, 6, 5, 4, 3, 2, 1, 7, 6, 5, 4, 3, 2, 1, 0];
const ILB: [i32; 32] = [
    2048, 2093, 2139, 2186, 2233, 2282, 2332, 2383, 2435, 2489, 2543, 2599, 2656, 2714, 2774, 2834,
    2896, 2960, 3025, 3091, 3158, 3228, 3298, 3371, 3444, 3520, 3597, 3676, 3756, 3838, 3922, 4008,
];
const WH: [i32; 3] = [0, -214, 798];
const RH2: [i32; 4] = [2, 1, 2, 1];
const QM2: [i32; 4] = [-7408, -1616, 7408, 1616];
const QM4: [i32; 16] = [
    0, -20456, -12896, -8968, -6288, -4240, -2584, -1200, 20456, 12896, 8968, 6288, 4240, 2584,
    1200, 0,
];
const QM5: [i32; 32] = [
    -280, -280, -23352, -17560, -14120, -11664, -9752, -8184, -6864, -5712, -4696, -3784, -2960,
    -2208, -1520, -880, 23352, 17560, 14120, 11664, 9752, 8184, 6864, 5712, 4696, 3784, 2960, 2208,
    1520, 880, 280, -280,
];
const QM6: [i32; 64] = [
    -136, -136, -136, -136, -24808, -21904, -19008, -16704, -14984, -13512, -12280, -11192, -10232,
    -9360, -8576, -7856, -7192, -6576, -6000, -5456, -4944, -4464, -4008, -3576, -3168, -2776,
    -2400, -2032, -1688, -1360, -1040, -728, 24808, 21904, 19008, 16704, 14984, 13512, 12280,
    11192, 10232, 9360, 8576, 7856, 7192, 6576, 6000, 5456, 4944, 4464, 4008, 3576, 3168, 2776,
    2400, 2032, 1688, 1360, 1040, 728, 432, 136, -432, -136,
];
const QMF_COEFFS: [i32; 12] = [3, -11, 12, 32, -210, 951, 3876, -805, 362, -156, 53, -11];

#[repr(C)]
pub enum Mode {
    Default = 0,
    SampleRate8000 = 1,
    Packed = 2,
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Options: u8 {
        const SAMPLE_RATE_8000 = 0b0001;
        const PACKED = 0b0010;
        const ITU_TEST_MODE = 0b0100;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BitPerSample {
    #[default]
    Bps48000 = 6,
    Bps56000 = 7,
    Bps64000 = 8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Band {
    pub s: i32,
    pub sp: i32,
    pub sz: i32,
    pub r: [i32; 3],
    pub a: [i32; 3],
    pub ap: [i32; 3],
    pub p: [i32; 3],
    pub d: [i32; 7],
    pub b: [i32; 7],
    pub bp: [i32; 7],
    pub sg: [i32; 7],
    pub nb: i32,
    pub det: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct G722Decoder {
    pub options: Options,
    pub bps: BitPerSample,
    pub x: [i32; 24],
    pub band: [Band; 2],
    pub in_buffer: u32,
    pub in_bits: i32,
    pub out_buffer: u32,
    pub out_bits: i32,
}

fn saturate(amp: i32) -> i16 {
    // 将 i32 转换为 i16
    let amp16 = amp as i16;

    // 检查是否发生溢出
    if amp == amp16 as i32 {
        return amp16;
    }

    // 处理溢出情况
    if amp > i16::MAX as i32 {
        return i16::MAX;
    }

    i16::MIN
}

#[allow(unused_assignments)]
fn block4(band: &mut Band, d: i32) {
    let mut wd1 = 0i32;
    let mut wd2 = 0i32;
    let mut wd3 = 0i32;

    /* Block 4, RECONS */
    band.d[0] = d;
    band.r[0] = saturate(band.s + d) as i32;

    /* Block 4, PARREC */
    band.p[0] = saturate(band.sz + d) as i32;

    /* Block 4, UPPOL2 */
    for i in 0..3 {
        band.sg[i] = band.p[i] >> 15;
    }
    wd1 = saturate(band.a[1] << 2) as i32;

    wd2 = if band.sg[0] == band.sg[1] { -wd1 } else { wd1 };
    if wd2 > 32767 {
        wd2 = 32767;
    }
    if band.sg[0] == band.sg[2] {
        wd3 = (wd2 >> 7) + 128;
    } else {
        wd3 = (wd2 >> 7) - 128;
    }
    wd3 += (band.a[2] * 32512) >> 15;
    wd3 = wd3.clamp(-12288, 12288);
    band.ap[2] = wd3;

    /* Block 4, UPPOL1 */
    band.sg[0] = band.p[0] >> 15;
    band.sg[1] = band.p[1] >> 15;
    wd1 = if band.sg[0] == band.sg[1] { 192 } else { -192 };
    wd2 = (band.a[1] * 32640) >> 15;

    band.ap[1] = saturate(wd1 + wd2) as i32;
    wd3 = saturate(15360 - band.ap[2]) as i32;
    if band.ap[1] > wd3 {
        band.ap[1] = wd3;
    } else if band.ap[1] < -wd3 {
        band.ap[1] = -wd3;
    }

    /* Block 4, UPZERO */
    wd1 = if d == 0 { 0 } else { 128 };
    band.sg[0] = d >> 15;
    for i in 1..7 {
        band.sg[i] = band.d[i] >> 15;
        wd2 = if band.sg[i] == band.sg[0] { wd1 } else { -wd1 };
        wd3 = (band.b[i] * 32640) >> 15;
        band.bp[i] = saturate(wd2 + wd3) as i32;
    }

    /* Block 4, DELAYA */
    for i in (1..7).rev() {
        band.d[i] = band.d[i - 1];
        band.b[i] = band.bp[i];
    }

    for i in (1..3).rev() {
        band.r[i] = band.r[i - 1];
        band.p[i] = band.p[i - 1];
        band.a[i] = band.ap[i];
    }

    /* Block 4, FILTEP */
    wd1 = saturate(band.r[1] + band.r[1]) as i32;
    wd1 = (band.a[1] * wd1) >> 15;
    wd2 = saturate(band.r[2] + band.r[2]) as i32;
    wd2 = (band.a[2] * wd2) >> 15;
    band.sp = saturate(wd1 + wd2) as i32;

    /* Block 4, FILTEZ */
    band.sz = 0;
    for i in (1..7).rev() {
        wd1 = saturate(band.d[i] + band.d[i]) as i32;
        band.sz += (band.b[i] * wd1) >> 15;
    }
    band.sz = saturate(band.sz) as i32;

    /* Block 4, PREDIC */
    band.s = saturate(band.sp + band.sz) as i32;
}

impl G722Decoder {
    pub fn new(bps: BitPerSample, options: Options) -> Self {
        let mut d = Self {
            bps,
            options,
            ..Default::default()
        };
        if d.options.contains(Options::PACKED) && d.bps != BitPerSample::Bps64000 {
            d.options.set(Options::PACKED, true);
        } else {
            d.options.set(Options::PACKED, false);
        }
        d.band[0].det = 32;
        d.band[1].det = 8;
        d
    }

    #[allow(unused_assignments)]
    pub fn decode<W: Write>(&mut self, data: &[u8], w: &mut W) -> std::io::Result<usize> {
        let mut dlowt = 0i32;
        let mut rlow = 0i32;
        let mut ihigh = 0i32;
        let mut dhigh = 0i32;
        let mut rhigh = 0i32;
        let mut xout1 = 0i32;
        let mut xout2 = 0i32;
        let mut wd1 = 0i32;
        let mut wd2 = 0i32;
        let mut wd3 = 0i32;
        let mut code = 0i32;
        let mut outlen = 0usize;

        for encoded in data.iter().copied() {
            if self.options.contains(Options::PACKED) {
                /* Unpack the code bits */
                if self.in_bits < self.bps as i32 {
                    self.in_buffer |= (encoded << self.in_bits) as u32;
                    self.in_bits += 8;
                }
                code = (self.in_buffer & ((1 << self.bps as u32) - 1)) as i32;
                self.in_buffer >>= self.bps as u32;
                self.in_bits -= self.bps as i32;
            } else {
                code = encoded as i32;
            }

            match self.bps {
                BitPerSample::Bps64000 => {
                    wd1 = code & 0x3F;
                    ihigh = (code >> 6) & 0x03;
                    wd2 = QM6[wd1 as usize];
                    wd1 >>= 2;
                }
                BitPerSample::Bps56000 => {
                    wd1 = code & 0x1F;
                    ihigh = (code >> 5) & 0x03;
                    wd2 = QM5[wd1 as usize];
                    wd1 >>= 1;
                }
                BitPerSample::Bps48000 => {
                    wd1 = code & 0x0F;
                    ihigh = (code >> 4) & 0x03;
                    wd2 = QM4[wd1 as usize];
                }
            }
            /* Block 5L, LOW BAND INVQBL */
            wd2 = (self.band[0].det * wd2) >> 15;
            /* Block 5L, RECONS */
            rlow = self.band[0].s + wd2;
            /* Block 6L, LIMIT */
            rlow = rlow.clamp(-16384, 16383);

            /* Block 2L, INVQAL */
            wd2 = QM4[wd1 as usize];
            dlowt = (self.band[0].det * wd2) >> 15;

            /* Block 3L, LOGSCL */
            wd2 = RL42[wd1 as usize];
            wd1 = (self.band[0].nb * 127) >> 7;
            wd1 += WL[wd2 as usize];
            wd1 = wd1.clamp(0, 18432);
            self.band[0].nb = wd1;

            /* Block 3L, SCALEL */
            wd1 = (self.band[0].nb >> 6) & 31;
            wd2 = 8 - (self.band[0].nb >> 11);
            wd3 = if wd2 < 0 {
                ILB[wd1 as usize] << -wd2
            } else {
                ILB[wd1 as usize] >> wd2
            };
            self.band[0].det = wd3 << 2;

            block4(&mut self.band[0], dlowt);

            if !self.options.contains(Options::SAMPLE_RATE_8000) {
                /* Block 2H, INVQAH */
                wd2 = QM2[ihigh as usize];
                dhigh = (self.band[1].det * wd2) >> 15;
                /* Block 5H, RECONS */
                rhigh = dhigh + self.band[1].s;
                /* Block 6H, LIMIT */
                rhigh = rhigh.clamp(-16384, 16383);
                /* Block 2H, INVQAH */
                wd2 = RH2[ihigh as usize];
                wd1 = (self.band[1].nb * 127) >> 7;
                wd1 += WH[wd2 as usize];
                wd1 = wd1.clamp(0, 22528);
                self.band[1].nb = wd1;

                /* Block 3H, SCALEH */
                wd1 = (self.band[1].nb >> 6) & 31;
                wd2 = 10 - (self.band[1].nb >> 11);
                let wd3 = if wd2 < 0 {
                    ILB[wd1 as usize] << -wd2
                } else {
                    ILB[wd1 as usize] >> wd2
                };
                self.band[1].det = wd3 << 2;

                block4(&mut self.band[1], dhigh);
            }

            if self.options.contains(Options::ITU_TEST_MODE) {
                w.write_all(&((rlow << 1) as u16).to_le_bytes())?;
                outlen += 2;
                w.write_all(&((rhigh << 1) as u16).to_le_bytes())?;
                outlen += 2;
            } else if self.options.contains(Options::SAMPLE_RATE_8000) {
                w.write_all(&((rlow << 1) as u16).to_le_bytes())?;
                outlen += 2;
            } else {
                /* Apply the receive QMF */
                for i in 0..22 {
                    self.x[i] = self.x[i + 2];
                }
                self.x[22] = rlow + rhigh;
                self.x[23] = rlow - rhigh;

                xout1 = 0;
                xout2 = 0;
                for i in 0..12 {
                    xout2 += self.x[2 * i] * QMF_COEFFS[i];
                    xout1 += self.x[2 * i + 1] * QMF_COEFFS[11 - i];
                }
                w.write_all(&saturate(xout1 >> 11).to_le_bytes())?;
                outlen += 2;
                w.write_all(&saturate(xout2 >> 11).to_le_bytes())?;
                outlen += 2;
            }
        }
        Ok(outlen)
    }
}

pub struct Decoder {
    decoded_data: AudioBuffer<i16>,
    params: CodecParameters,
    raw: G722Decoder,
}

impl D for Decoder {
    fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self>
    where
        Self: Sized,
    {
        let bps = match params.bits_per_sample {
            Some(48000) => BitPerSample::Bps48000,
            Some(56000) => BitPerSample::Bps56000,
            Some(64000) => BitPerSample::Bps64000,
            Some(_) | None => BitPerSample::Bps64000,
        };
        let mut options = Options::default();
        let sr = params.sample_rate.unwrap_or(16000);
        if sr == 8000 {
            options.set(Options::SAMPLE_RATE_8000, true);
        }

        Ok(Self {
            decoded_data: AudioBuffer::new(
                sr as u64 / 50,
                SignalSpec::new(sr, Channels::FRONT_CENTRE),
            ),
            params: params.clone(),
            raw: G722Decoder::new(bps, options),
        })
    }

    fn reset(&mut self) {
        self.raw = G722Decoder::new(self.raw.bps, self.raw.options);
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[support_codec!(CODEC_TYPE_G722, "g722", "G.722")]
    }

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef> {
        self.decoded_data.clear();
        self.decoded_data
            .render_reserved(Some(self.params.sample_rate.unwrap_or(16000) as usize / 50));

        let mut a: &mut [u8] = cast_slice_mut(self.decoded_data.chan_mut(0));
        self.raw.decode(&packet.data, &mut a)?;

        Ok(self.decoded_data.as_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        Default::default()
    }

    fn last_decoded(&self) -> AudioBufferRef {
        self.decoded_data.as_audio_buffer_ref()
    }
}
