use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::bail;
use matrix_sdk::ruma::OwnedUserId;
use serde::{Deserialize, Serialize};

use super::CRATE_NAME;

fn config_path() -> io::Result<PathBuf> {
    match env::var("MN_CONFIG") {
        Ok(path) => Ok(path.into()),
        Err(_) => {
            let xdg_dirs = xdg::BaseDirectories::with_prefix(CRATE_NAME)?;
            xdg_dirs.place_config_file("config.toml")
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Config {
    pub(crate) user_id: OwnedUserId,
    pub(crate) device_name: Option<String>,
}

impl Config {
    pub(crate) fn exists() -> io::Result<bool> {
        config_path()?.try_exists()
    }

    pub(crate) fn load() -> anyhow::Result<Self> {
        let raw = fs::read_to_string(config_path()?)?;
        if raw.is_empty() {
            bail!("empty file");
        }

        Ok(toml::from_str(&raw)?)
    }

    pub(crate) fn dump(&self) -> anyhow::Result<()> {
        let raw = toml::to_string(&self)?;
        fs::write(config_path()?, &raw)?;
        Ok(())
    }
}
