use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use bindgen::callbacks::{MacroParsingBehavior, ParseCallbacks};

/// https://github.com/rust-lang/rust-bindgen/issues/687#issuecomment-450750547
#[derive(Debug)]
pub struct IgnoreMacros(pub HashSet<&'static str>);

impl ParseCallbacks for IgnoreMacros {
    fn will_parse_macro(&self, name: &str) -> MacroParsingBehavior {
        if self.0.contains(name) {
            MacroParsingBehavior::Ignore
        } else {
            MacroParsingBehavior::Default
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub enum LinkType {
    #[default]
    Dynamic,
    Static,
}

impl std::fmt::Display for LinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dynamic => f.write_str("dylib"),
            Self::Static => f.write_str("static"),
        }
    }
}

pub struct Library {
    pub name: String,
    /// Some library may not follows the semver way, so we use String here
    pub version: Option<String>,
    pub link_type: LinkType,
    /// Specify extra include path
    pub inc_paths: Vec<PathBuf>,
    /// Specify extra link path
    pub link_paths: Vec<PathBuf>,
    /// Install root location, e.g. PCRE_ROOT, LLHTTP_ROOT
    pub root_env: String,
    /// Whether needs to link to static std c++ library
    pub static_link_std_cpp: bool,
}

impl Library {
    pub fn new(name: String, root_env: String) -> Self {
        Self {
            name,
            version: None,
            link_type: LinkType::Dynamic,
            inc_paths: vec![],
            link_paths: vec![],
            root_env,
            static_link_std_cpp: false,
        }
    }
}

/// Find library header/library/pkgconfig location
pub fn find_lib(library: &mut Library) -> Result<()> {
    cargo_emit::rerun_if_env_changed!(library.root_env);

    if let Ok(prefix) = env::var(&library.root_env) {
        let prefix = Path::new(&prefix);
        library
            .inc_paths
            .push(PathBuf::from(&prefix).join("include"));
        let mut link_paths = vec![];
        for sub_dir in ["lib", "lib64"] {
            let link_path = prefix.join(sub_dir);
            link_paths.push(link_path);
        }

        if !prefix.exists() || !prefix.is_dir() {
            bail!(
                "{} should point to a directory that exists.",
                library.root_env
            );
        }

        if link_paths.iter().all(|p| !p.exists()) {
            bail!("no sub directory found in `${}`.", library.root_env);
        }
        if link_paths.iter().all(|p| !p.is_dir()) {
            bail!("no sub directory found in `${}`.", library.root_env);
        }

        for p in link_paths {
            if p.exists() && p.is_dir() {
                cargo_emit::rustc_link_search!(p.to_string_lossy() => "native");
            }
        }

        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
        let std_link = if target_os == "macos" {
            "c++"
        } else {
            "stdc++"
        };
        if library.static_link_std_cpp {
            cargo_emit::rustc_link_lib!(std_link => "static:-bundle");
        } else {
            cargo_emit::rustc_link_lib!(std_link);
        }

        cargo_emit::rustc_link_lib!(library.name => library.link_type.to_string());
    }

    Ok(())
}
