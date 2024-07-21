#[allow(clippy::all)]
#[allow(warnings)]
mod bindings {
    #[cfg(feature = "gen")]
    include!(concat!(env!("OUT_DIR"), "/libg7221_sys.rs"));
}

pub use bindings::*;
