mod config;
mod shunt;

use config::Config;
use shunt::go;
use std::{io::Read, path::Path};

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    if args.len() != 1 {
        anyhow::bail!("expected path to single configuration file")
    }

    let config_path = args.next().context("unable to read argv")?;
    let config_path = Path::new(&config_path);

    let config_path = config_path
        .canonicalize()
        .context("directory of config not canonicalizable")?;

    let config_dir = config_path
        .parent()
        .context("config file had no parent directory")?
        .to_owned();

    let config = std::fs::File::open(&config_path)
        .and_then(|mut f| {
            let mut s = String::new();
            f.read_to_string(&mut s)?;
            Ok(s)
        })
        .context(format!("could not open config: {:?}", &config_path))?;

    // Everything should happen relative to the config dir.
    std::env::set_current_dir(config_dir).unwrap();

    let config: Config = match config_path.extension().and_then(|s| s.to_str()) {
        Some("json" | "json5") => {
            json5::from_str(&config).context("could not parse JSON config file")
        }
        Some(ext) => anyhow::bail!("unknown file extension for config file: {ext}"),
        None => anyhow::bail!("could not recognise extension for config file"),
    }?;

    go(config)
}
