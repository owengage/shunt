use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    fs::File,
    hash::{Hash, Hasher},
    io::{BufRead, BufReader, Read},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use std::io::Write;

use anyhow::Context;
use serde::Deserialize;
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    iterator::Signals,
};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, StandardStreamLock, WriteColor};

#[derive(Debug, Deserialize)]
struct Config {
    commands: HashMap<String, CommandConfig>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum AutoBool {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone)]
struct CommandConfig {
    argv: Vec<String>,
    workdir: PathBuf,
    tty: AutoBool,
}

#[derive(Debug, Clone)]
struct CommandInfo {
    name: String,
    color: Option<ColorSpec>,
}

impl<'de> Deserialize<'de> for CommandConfig {
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
            CommandConf::Split(argv) => CommandConfig {
                argv,
                tty: AutoBool::Auto,
                workdir: cwd,
            },
            CommandConf::Full { argv, tty, workdir } => CommandConfig {
                argv,
                tty: tty.unwrap_or(AutoBool::Auto),
                workdir: cwd.join(workdir.unwrap_or_else(|| PathBuf::from("."))),
            },
        })
    }
}

fn handle_stdout(h: &mut Handle, info: &CommandInfo, out: impl Read) {
    let cmd_stdout = BufReader::new(out);
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);

    for line in cmd_stdout.lines().flatten() {
        prefix_write(&mut stdout, info, &line);
    }

    match h.child.wait() {
        Ok(status) => {
            let mut stdout = stdout.lock();

            colored_write(
                &mut stdout,
                &info.color,
                &format!("\"{}\" {}.\n", info.name, status),
            );
        }
        Err(e) => println!("{} failed to be waited on: {}", info.name, e),
    }
}

fn go(config: Config) -> anyhow::Result<()> {
    let mut signals = Signals::new([SIGTERM, SIGINT])?;
    let handle = signals.handle();

    let mut handles = config
        .commands
        .iter()
        .map(|(name, info)| start_command(name, info))
        .collect::<Vec<_>>();

    let ids: Vec<_> = handles.iter().flatten().map(|h| h.child.id()).collect();

    std::thread::spawn(move || {
        // We set the parent process of each child to itself to give each it's
        // own process group. This means ^C to gather isn't automatically sent
        // to all child processes. We manually forward to signal to each child.
        //
        // Without the isolating process groups, we'd double-SIGINT each child
        // process on ^C, potentially causing issues.
        for signal in &mut signals {
            for id in &ids {
                unsafe { nix::libc::kill(*id as i32, signal) };
            }
        }
    });

    std::thread::scope(|s| {
        for h in &mut handles {
            let h = match h {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("{e:?}");
                    continue;
                }
            };

            if let Some(tty_master) = h.tty_master.take() {
                let info = h.info.clone();
                s.spawn(move || handle_stdout(h, &info, tty_master));
            } else {
                let cmd_stdout = h.child.stdout.take().unwrap();
                let cmd_stderr = h.child.stderr.take().unwrap();
                let info = h.info.clone();

                // stderr is handled separately if we're not in a pseudo tty.
                s.spawn(move || {
                    let cmd_stderr = BufReader::new(cmd_stderr);
                    let mut stderr = StandardStream::stderr(ColorChoice::Auto);

                    for line in cmd_stderr.lines().flatten() {
                        prefix_write(&mut stderr, &info, &line);
                    }
                });

                let info = h.info.clone();
                s.spawn(move || handle_stdout(h, &info, cmd_stdout));
            };
        }
    });
    handle.close();
    Ok(())
}

fn colored_write(stdout: &mut StandardStreamLock, color: &Option<ColorSpec>, s: &str) {
    if let Some(color) = color {
        stdout.set_color(color).unwrap();
    }
    write!(stdout, "{}", s).unwrap();
    if color.is_some() {
        stdout.reset().unwrap();
    }
}

fn prefix_write(stream: &mut StandardStream, info: &CommandInfo, s: &str) {
    let mut stream = stream.lock();
    colored_write(&mut stream, &info.color, &format!("[{}] ", &info.name));
    writeln!(&mut stream, "{}", s).unwrap();
}

fn make_color(c: Color) -> ColorSpec {
    let mut col = ColorSpec::new();
    col.set_fg(Some(c));
    col
}

fn start_command(name: &str, cmd_config: &CommandConfig) -> anyhow::Result<Handle> {
    // Are *we* attached to a TTY?
    let our_stdout = std::io::stdout().as_raw_fd();
    let is_tty = nix::unistd::isatty(our_stdout).unwrap();

    let wrap_tty = match cmd_config.tty {
        AutoBool::Auto => is_tty,
        AutoBool::Always => true,
        AutoBool::Never => false,
    };

    let mut tty_master = None;

    let (stdout, stderr) = if wrap_tty {
        let pty = nix::pty::openpty(None, None).unwrap();
        tty_master = Some(unsafe { File::from_raw_fd(pty.master) });
        let slave = unsafe { File::from_raw_fd(pty.slave) };
        let slave2 = slave.try_clone().unwrap();
        (Stdio::from(slave), Stdio::from(slave2))
    } else {
        (Stdio::piped(), Stdio::piped())
    };

    let cmd = Command::new(
        cmd_config
            .argv
            .get(0)
            .context(format!("command \"{}\" was empty", name))?,
    )
    .args(&cmd_config.argv[1..])
    // Set the parent process group to the child's process itself. This allows
    // us to better control when children receive signals.
    .process_group(0)
    .stdout(stdout)
    .stderr(stderr)
    .stdin(Stdio::null())
    .current_dir(&cmd_config.workdir)
    .spawn()
    .context(format!("command \"{}\" failed to spawn", name))?;

    Ok(Handle {
        info: CommandInfo {
            name: name.to_owned(),
            color: if is_tty {
                Some(pick_color(name, cmd_config))
            } else {
                None
            },
        },
        child: cmd,
        tty_master,
    })
}

fn pick_color(name: &str, _: &CommandConfig) -> ColorSpec {
    let colors = [
        make_color(Color::Green),
        make_color(Color::Red),
        make_color(Color::Cyan),
        make_color(Color::Magenta),
        make_color(Color::Yellow),
    ];

    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let i = hasher.finish() as usize % colors.len();
    colors[i].clone()
}

#[derive(Debug)]
struct Handle {
    info: CommandInfo,
    child: Child,
    tty_master: Option<File>,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    if args.len() != 1 {
        anyhow::bail!("expected path to single configuration file")
    }

    let config_path = args.next().context("unable to read argv")?;
    let config_path = Path::new(&config_path);
    let config = std::fs::File::open(config_path)
        .and_then(|mut f| {
            let mut s = String::new();
            f.read_to_string(&mut s)?;
            Ok(s)
        })
        .context(format!("could not open config: {:?}", config_path))?;

    let config: Config = match config_path.extension().and_then(|s| s.to_str()) {
        Some("json" | "json5") => {
            json5::from_str(&config).context("could not parse JSON config file")
        }
        Some(ext) => anyhow::bail!("unknown file extension for config file: {ext}"),
        None => anyhow::bail!("could not recognise extension for config file"),
    }?;

    go(config)
}
