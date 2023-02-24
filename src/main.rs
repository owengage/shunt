mod guts;
mod shunt;

use clap::Parser;
use guts::go;
use shunt::Shunt;
use std::{
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::Context;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    shunt_file: PathBuf,
    /// Exclude the named commands, can be specified multiple times.
    #[arg(short = 'e', long)]
    exclude: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config_path = Path::new(&args.shunt_file);

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

    let mut config: Shunt = match config_path.extension().and_then(|s| s.to_str()) {
        Some("json" | "json5") => {
            json5::from_str(&config).context("could not parse JSON config file")
        }
        Some(ext) => anyhow::bail!("unknown file extension for config file: {ext}"),
        None => anyhow::bail!("could not recognise extension for config file"),
    }?;

    // Remove commands that have been excluded.
    config
        .commands
        .retain(|name, _| !args.exclude.contains(name));

    go(config)
}
