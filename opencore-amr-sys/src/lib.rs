#[cfg(feature = "gen")]
include!(concat!(env!("OUT_DIR"), "/opencore_amr_sys.rs"));

#[cfg(all(not(feature = "gen"), target_os = "macos", target_arch = "aarch64"))]
include!("macos_aarch64.rs");
