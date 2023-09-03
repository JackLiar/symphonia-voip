#[cfg(feature = "gen")]
include!(concat!(env!("OUT_DIR"), "/evs_codec_sys.rs"));

#[cfg(all(not(feature = "gen"), target_os = "macos"))]
include!("macos_x86_64.rs");

#[cfg(feature = "floating-point")]
macro_rules! EVS {
    ($field_name:ident) => {
        $field_name
    };
}

// impl Decoder_State {
//     pub fn reset_on_mime(&mut self) {
//         self.BER_detect = 0;
//         self.bfi = 0;
//         self.mdct_sw_enable = 0;
//         self.mdct_sw = 0;
//         unsafe { reset_indices_dec(self) }
//     }
// }
