use anyhow::{anyhow, Result};

use sys_builder::{find_lib, IgnoreMacros, Library};

#[cfg(feature = "gen")]
fn gen() -> Result<()> {
    use std::env;
    use std::path::Path;

    let ignored_macros = IgnoreMacros(
        vec![
            "FP_INFINITE",
            "FP_NAN",
            "FP_NORMAL",
            "FP_SUBNORMAL",
            "FP_ZERO",
        ]
        .into_iter()
        .collect(),
    );

    let mut library = Library::new("g722_1".to_string(), "LIBG7221_ROOT".to_string());
    find_lib(&mut library)
        .map_err(|e| anyhow!("Failed to find {} library, {}", library.name, e))?;
    let out_dir = env::var("OUT_DIR")?;
    let out_path = Path::new(&out_dir).join("libg7221_sys.rs");

    let mut bindings = bindgen::builder()
        .default_macro_constant_type(bindgen::MacroTypeVariation::Signed)
        .disable_nested_struct_naming()
        .trust_clang_mangling(false)
        .derive_default(true);

    if let Ok(cpath_dir) = env::var("CPATH") {
        bindings = bindings.clang_arg(format!("-I{}", cpath_dir))
    }

    bindings = bindings.clang_args(
        library
            .inc_paths
            .iter()
            .map(|p| format!("-I{}", p.display())),
    );

    if let Some(hdr) = library
        .inc_paths
        .iter()
        .map(|p| p.join("g722_1.h"))
        .find(|p| p.exists())
    {
        bindings = bindings.header(hdr.display().to_string());
    }

    bindings
        .parse_callbacks(Box::new(ignored_macros))
        .layout_tests(false)
        .generate()
        .unwrap_or_else(|e| panic!("could not run bindgen on header src/amrwb.h, {}", e))
        .write_to_file(&out_path)
        .unwrap_or_else(|e| panic!("Could not write to {:?}, {}", out_path, e));
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gen")]
    gen()?;
    cargo_emit::rustc_link_lib!("g722_1" => "static");

    Ok(())
}
