use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Opening file errored with {0}")]
    FileOpen(#[from] super::utils::Error),
    #[error("Reading file errored with {0}")]
    ReadFileFailed(#[from] std::io::Error),
    #[error("Failed to parse TOML with {0}")]
    FailedToParseTOML(#[from] toml::de::Error),
    #[error("File path is not absolute")]
    PathNotAbsolute,
    #[error("Path contains disallowed component: {0}")]
    PathContainsDisallowedComponents(String),
    #[error("Config name must be a single file name component")]
    InvalidConfigName,
    #[error("Config name must contains only one component")]
    ComponentsIsNotLength1,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct DirOverrideConfig {
    source: PathBuf,
    #[serde(default = "default_overlayfs")]
    overlayfs: bool,
}

fn default_overlayfs() -> bool {
    true
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Config {
    #[serde(default)]
    overrides: std::collections::HashMap<PathBuf, DirOverrideConfig>,
}

#[derive(Debug)]
pub struct FileOverride {
    pub source: std::fs::File,
    pub target: std::os::fd::OwnedFd,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
}

#[derive(Debug)]
pub struct DirOverride {
    pub source: nix::dir::Dir,
    pub target: nix::dir::Dir,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub overlayfs: bool,
}

pub enum MntOverride {
    File(FileOverride),
    Dir(DirOverride),
}

pub fn parse_config(config_name: &std::path::Path) -> Result<Vec<MntOverride>, Error> {
    use crate::utils::{open_config_file, validate_override};
    use itertools::Itertools;
    use rustix::path::Arg;
    use std::io::Read;
    use std::path::Component;

    let config_name = match config_name.components().exactly_one() {
        Ok(Component::Normal(name)) => Component::Normal(name),
        Ok(_) => {
            return Err(Error::InvalidConfigName);
        }
        Err(_) => {
            return Err(Error::ComponentsIsNotLength1);
        }
    };

    let mut config_file = open_config_file(&config_name)?;
    let mut config_contents = String::new();
    config_file.read_to_string(&mut config_contents)?;

    let config: Config = toml::from_str(&config_contents)?;

    config
        .overrides
        .into_iter()
        .map(|(target, source)| {
            if !target.is_absolute() || !source.source.is_absolute() {
                return Err(Error::PathNotAbsolute);
            }

            for path in [&target, &source.source] {
                if let Some(disallowed_component) = path.components().find(|comp| {
                    matches!(
                        *comp,
                        Component::Prefix(_) | Component::CurDir | Component::ParentDir
                    )
                }) {
                    return Err(Error::PathContainsDisallowedComponents(
                        disallowed_component.to_string_lossy().to_string(),
                    ));
                }
            }

            Ok((target, source))
        })
        .collect::<Result<Vec<(PathBuf, DirOverrideConfig)>, Error>>()?
        .into_iter()
        .map(
            |(target, source)| match validate_override(source.source, target)? {
                MntOverride::Dir(mut dir) => {
                    dir.overlayfs = source.overlayfs;
                    Ok(MntOverride::Dir(dir))
                }
                MntOverride::File(file) => Ok(MntOverride::File(file)),
            },
        )
        .collect::<Result<Vec<MntOverride>, Error>>()
}
