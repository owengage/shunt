use std::{collections::HashMap, path::PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Shunt {
    pub commands: HashMap<String, ShuntCommand>,
}

#[derive(Debug, Clone)]
pub struct ShuntCommand {
    pub argv: Vec<String>,
    pub workdir: PathBuf,
    pub tty: AutoBool,
}

impl<'de> Deserialize<'de> for ShuntCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum CommandConf {
            Split(Vec<String>),
            Full {
                argv: Vec<String>,
                tty: Option<AutoBool>,
                workdir: Option<PathBuf>,
            },
        }

        let inner = CommandConf::deserialize(deserializer)?;

        let cwd = match std::env::current_dir() {
            Ok(cwd) => cwd,
            Err(_) => {
                return Err(serde::de::Error::custom(
                    "could not access current working directory",
                ))
            }
        };

        Ok(match inner {
            CommandConf::Split(argv) => ShuntCommand {
                argv,
                tty: AutoBool::Auto,
                workdir: cwd,
            },
            CommandConf::Full { argv, tty, workdir } => ShuntCommand {
                argv,
                tty: tty.unwrap_or(AutoBool::Auto),
                workdir: cwd.join(workdir.unwrap_or_else(|| PathBuf::from("."))),
            },
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutoBool {
    Auto,
    Always,
    Never,
}
