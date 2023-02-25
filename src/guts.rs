use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    process::{Child, Command, Stdio},
    sync::atomic::{self, AtomicU64},
};

use anyhow::Context;
use nix::libc::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, StandardStreamLock, WriteColor};

use crate::shunt::{AutoBool, Shunt, ShuntCommand};

#[derive(Debug, Clone)]
struct CommandInfo {
    name: String,
    color: Option<ColorSpec>,
}

fn handle_output(info: &CommandInfo, out: impl Read) {
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);

    let br = BufReader::new(out);

    for line in br.lines() {
        match line {
            Ok(line) => prefix_write(&mut stdout, info, &line),
            Err(_) => {
                // This is the expected way to exit, the output we're reading
                // got closed.
                break;
            }
        }
    }
}

pub fn go(config: Shunt) -> anyhow::Result<()> {
    let mut signals = Signals::new([SIGTERM, SIGINT])?;
    let handle = signals.handle();

    let mut handles = config
        .commands
        .iter()
        .map(|(name, info)| start_command(name, info))
        .collect::<Vec<_>>();

    std::thread::spawn(move || {
        for signal in &mut signals {
            println!("shunt received signal {}", signal);
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

            // if let Some(tty_master) = h.tty_master.take() {
            let info = h.info.clone();
            let tty = h.tty_master.try_clone().unwrap();

            s.spawn(move || handle_wait(h));
            s.spawn(move || handle_output(&info, tty));
        }
    });
    handle.close();
    Ok(())
}

fn handle_wait(h: &mut Handle) {
    match h.child.wait() {
        Ok(status) => {
            println!("{} finished: {}", h.info.name, status);
        }
        Err(e) => println!("{} failed to be waited on: {}", h.info.name, e),
    }
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

fn start_command(name: &str, cmd_config: &ShuntCommand) -> anyhow::Result<Handle> {
    // Are *we* attached to a TTY?
    let our_stdout = std::io::stdout().as_raw_fd();
    let is_tty = nix::unistd::isatty(our_stdout).unwrap();

    let wrap_tty = match cmd_config.tty {
        AutoBool::Auto => is_tty,
        AutoBool::Always => true,
        AutoBool::Never => false,
    };

    let (read_end, write_end) = if wrap_tty {
        let pty = nix::pty::openpty(None, None).unwrap();
        (pty.master, pty.slave)
    } else {
        // Pipe stdout and stderr to the same place.
        let mut fds = [-1i32; 2];
        unsafe { nix::libc::pipe(&mut (fds[0]) as *mut _) };
        (fds[0], fds[1])
    };

    let (tty, stdout, stderr) = unsafe {
        (
            File::from_raw_fd(read_end),
            Stdio::from_raw_fd(write_end),
            Stdio::from_raw_fd(write_end),
        )
    };

    let env_updates: HashMap<_, _> = cmd_config
        .env
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
        .collect();

    let removed_env: Vec<_> = cmd_config
        .env
        .iter()
        .filter_map(|(k, v)| if v.is_none() { Some(k) } else { None })
        .collect();

    let mut cmd = Command::new(
        cmd_config
            .argv
            .get(0)
            .context(format!("command \"{}\" was empty", name))?,
    );

    cmd.args(&cmd_config.argv[1..])
        .current_dir(&cmd_config.workdir)
        .stdout(stdout)
        .stderr(stderr)
        .stdin(Stdio::null())
        .envs(env_updates);

    for k in removed_env {
        cmd.env_remove(k);
    }

    let cmd = cmd
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
        tty_master: tty,
    })
}

static COLOR_CYCLE: AtomicU64 = AtomicU64::new(0);

fn pick_color(_: &str, _: &ShuntCommand) -> ColorSpec {
    let colors = [
        Color::Green,
        Color::Red,
        Color::Cyan,
        Color::Magenta,
        Color::Yellow,
    ];

    let i = COLOR_CYCLE.fetch_add(1, atomic::Ordering::Relaxed);
    make_color(colors[i as usize])
}

#[derive(Debug)]
struct Handle {
    info: CommandInfo,
    child: Child,
    tty_master: File,
}
