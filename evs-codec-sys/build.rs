use std::env;
use std::ffi::OsStr;
use std::fs::read_dir;
use std::path::Path;

use anyhow::{anyhow, Result};

#[cfg(feature = "gen")]
fn gen() -> Result<()> {
    let out_dir = env::var("OUT_DIR")?;
    let out_path = Path::new(&out_dir).join("evs_codec_sys.rs");

    let bindings = bindgen::builder()
        .default_macro_constant_type(bindgen::MacroTypeVariation::Signed)
        .disable_nested_struct_naming()
        .trust_clang_mangling(false)
        .clang_arg("-I.")
        .clang_arg("-I./c-code/lib_com")
        .clang_arg("-I./c-code/lib_dec")
        .clang_arg("-I./c-code/lib_enc")
        .derive_default(true);

    #[cfg(feature = "floating-point")]
    let bindings = bindings.header("src/evs_codec.h");

    bindings
        .layout_tests(false)
        .generate()
        .unwrap_or_else(|e| panic!("could not run bindgen on header src/evs_codec.h, {}", e))
        .write_to_file(&out_path)
        .unwrap_or_else(|e| panic!("Could not write to {:?}, {}", out_path, e));
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "floating-point")]
    let dirs = ["c-code/lib_com", "c-code/lib_dec", "c-code/lib_enc"];

    for dir in dirs {
        println!("cargo:rerun-if-changed={}", dir);
    }
    let mut cfg = cc::Build::new();
    let mut files = vec![];
    for dir in dirs {
        for entry in read_dir(dir).map_err(|e| anyhow!("read dir {:?} failed: {e}", dir))? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            if path == Path::new("encoder.c") || path == Path::new("decoder.c") {
                continue;
            };
            if path.extension() != Some(OsStr::new("c")) {
                continue;
            }
            files.push(path);
        }
    }
    cfg.files(files)
        .flag("-pedantic")
        .flag("-Wcast-qual")
        .flag("-Wno-long-long")
        .flag("-Wpointer-arith")
        .flag("-Wstrict-prototypes")
        .flag("-Wmissing-prototypes")
        .flag("-Werror-implicit-function-declaration")
        .includes(dirs)
        .warnings(false);

    cfg.compile("evs");
    #[cfg(feature = "gen")]
    gen()?;
    Ok(())
}
