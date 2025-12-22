use crate::utils::{OpenFileOverrideType, open_dir, open_override_file};
use nix::fcntl::OFlag;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Opening file errored with {0}")]
    FileOpen(#[from] super::utils::Error),
    #[error("Reading file errored with {0}")]
    ReadFileFailed(#[from] std::io::Error),
    #[error("Failed to parse TOML with {0}")]
    FailedToParseTOML(#[from] toml::de::Error),
    #[error("File path is not absolute")]
    FilePathNotAbsolute,
    #[error("Config name must be a single file name component")]
    InvalidConfigName,
    #[error("Config name must contains only one component")]
    ComponentsIsNotLength1,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct DirOverrideConfig {
    source: String,
    #[serde(default = "default_overlayfs")]
    overlayfs: bool,
}

fn default_overlayfs() -> bool {
    true
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Config {
    #[serde(default)]
    file_overrides: std::collections::HashMap<String, String>,

    #[serde(default)]
    dir_overrides: std::collections::HashMap<String, DirOverrideConfig>,
}

#[derive(Debug)]
pub struct FileOverride {
    pub source: std::fs::File,
    pub target: std::fs::File,
    pub source_path: std::path::PathBuf,
    pub target_path: std::path::PathBuf,
}

#[derive(Debug)]
pub struct DirOverride {
    pub source: nix::dir::Dir,
    pub target: nix::dir::Dir,
    pub source_path: std::path::PathBuf,
    pub target_path: std::path::PathBuf,
    pub overlayfs: bool,
}

pub struct Input {
    pub file_overrides: Vec<FileOverride>,
    pub dir_overrides: Vec<DirOverride>,
}

pub fn parse_config(config_name: &std::path::Path) -> Result<Input, Error> {
    use crate::utils;
    use itertools::Itertools;
    use std::io::Read;
    use std::path::Component;
    use std::path::PathBuf;
    use utils::DirType;

    let config_name = match config_name.components().exactly_one() {
        Ok(Component::Normal(name)) => Component::Normal(name),
        Ok(_) => {
            return Err(Error::InvalidConfigName);
        }
        Err(_) => {
            return Err(Error::ComponentsIsNotLength1);
        }
    };

    let mut config_file = utils::open_config_file(&config_name)?;
    let mut config_contents = String::new();
    config_file.read_to_string(&mut config_contents)?;

    let config: Config = toml::from_str(&config_contents)?;

    let file_overrides = config
        .file_overrides
        .iter()
        .map(|(target, source)| {
            let r#override = (PathBuf::from(target), PathBuf::from(source));

            if !r#override.0.is_absolute() || !r#override.1.is_absolute() {
                Err(Error::FilePathNotAbsolute)
            } else {
                Ok(r#override)
            }
        })
        .collect::<Result<Vec<(PathBuf, PathBuf)>, Error>>()?
        .iter()
        .map(|(target, source)| {
            Ok(FileOverride {
                source: open_override_file(source, OFlag::O_PATH, &OpenFileOverrideType::Source)?,
                target: open_override_file(target, OFlag::O_PATH, &OpenFileOverrideType::Target)?,
                source_path: source.clone(),
                target_path: target.clone(),
            })
        })
        .collect::<Result<Vec<FileOverride>, Error>>()?;

    let dir_overrides = config
        .dir_overrides
        .iter()
        .map(|(target, dir_config)| {
            let r#override = (PathBuf::from(target), PathBuf::from(&dir_config.source));
            if !r#override.0.is_absolute() || !r#override.1.is_absolute() {
                Err(Error::FilePathNotAbsolute)
            } else {
                Ok((r#override, dir_config.overlayfs))
            }
        })
        .collect::<Result<Vec<((PathBuf, PathBuf), bool)>, Error>>()?
        .into_iter()
        .map(|((target_path, source_path), overlayfs)| {
            Ok(DirOverride {
                source: open_dir(&source_path, &DirType::Source)?,
                target: open_dir(&target_path, &DirType::Target)?,
                source_path,
                target_path,
                overlayfs,
            })
        })
        .collect::<Result<Vec<DirOverride>, Error>>()?;

    Ok(Input {
        file_overrides,
        dir_overrides,
    })
}
