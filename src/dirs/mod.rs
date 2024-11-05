use std::path::Path;
use std::path::PathBuf;

use miette::bail;
use miette::IntoDiagnostic;

const DEFAULT_PATH_NAME: &str = "schultz";

fn default_root_dir() -> miette::Result<PathBuf> {
    if let Some(home) = directories::UserDirs::new() {
        return Ok(home.home_dir().join(DEFAULT_PATH_NAME));
    }

    bail!("Use root_dir parameter or env");
}

pub fn ensure_root_dir(explicit: Option<&Path>) -> miette::Result<PathBuf> {
    let defined = explicit.map(|p| p.join(DEFAULT_PATH_NAME)).unwrap_or(default_root_dir()?);

    std::fs::create_dir_all(&defined).into_diagnostic()?;

    Ok(defined)
}

pub struct Dirs {
    pub root_dir: PathBuf,
}

impl Dirs {
    pub fn try_new(root_dir: Option<&Path>) -> miette::Result<Self> {
        let root_dir = ensure_root_dir(root_dir)?;

        Ok(Self { root_dir })
    }
}