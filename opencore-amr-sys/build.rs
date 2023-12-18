use anyhow::Result;

fn main() -> Result<()> {
    #[cfg(feature = "gen")]
    {
        use std::env;
        use std::path::Path;
        let out_dir = env::var("OUT_DIR")?;
        let out_path = Path::new(&out_dir).join("opencore_amr_sys.rs");

        let cpath_dir = env::var("CPATH")?;
        let bindings = bindgen::builder()
            .default_macro_constant_type(bindgen::MacroTypeVariation::Signed)
            .disable_nested_struct_naming()
            .trust_clang_mangling(false)
            .clang_arg(format!("-I{}", cpath_dir))
            .derive_default(true);
        let bindings = bindings.header("src/amrwb.h");

        bindings
            .layout_tests(false)
            .generate()
            .unwrap_or_else(|e| panic!("could not run bindgen on header src/amrwb.h, {}", e))
            .write_to_file(&out_path)
            .unwrap_or_else(|e| panic!("Could not write to {:?}, {}", out_path, e));
    }

    cargo_emit::rustc_link_lib!("opencore-amrnb");
    cargo_emit::rustc_link_lib!("opencore-amrwb");

    Ok(())
}
