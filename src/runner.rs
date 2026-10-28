use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

struct Args {
    core: PathBuf,
    config: PathBuf,
    stop: PathBuf,
    log: PathBuf,
    parent: u32,
}

fn parse_args() -> Option<Args> {
    let mut core = None;
    let mut config = None;
    let mut stop = None;
    let mut log = None;
    let mut parent = None;
    let mut args = std::env::args().skip(2);
    while let Some(flag) = args.next() {
        let value = args.next()?;
        match flag.as_str() {
            "--core" => core = Some(PathBuf::from(value)),
            "--config" => config = Some(PathBuf::from(value)),
            "--stop" => stop = Some(PathBuf::from(value)),
            "--log" => log = Some(PathBuf::from(value)),
            "--parent" => parent = value.parse().ok(),
            _ => return None,
        }
    }
    Some(Args {
        core: core?,
        config: config?,
        stop: stop?,
        log: log?,
        parent: parent?,
    })
}

fn append_log(path: &PathBuf, line: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

pub const FLAG: &str = "--prime-runner";

pub fn run_if_requested() {
    if std::env::args().nth(1).as_deref() == Some(FLAG) {
        platform::detach_console();
        run();
        std::process::exit(0);
    }
}

fn run() {
    let Some(args) = parse_args() else {
        std::process::exit(2);
    };
    let config = std::fs::read(&args.config);
    let _ = std::fs::remove_file(&args.config);
    let _ = std::fs::write(&args.log, b"");
    let Ok(config) = config else {
        append_log(&args.log, "runner: config is missing");
        std::process::exit(3);
    };

    platform::allow_through_firewall(&args.core);

    let mut command = Command::new(&args.core);
    command
        .args(["run", "-c", "stdin"])
        .current_dir(args.core.parent().unwrap_or(&args.core))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    platform::hide_window(&mut command);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            append_log(&args.log, &format!("runner: failed to start core: {error}"));
            std::process::exit(4);
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&config);
    }
    if let Some(stderr) = child.stderr.take() {
        let log = args.log.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                append_log(&log, &line);
            }
        });
    }

    loop {
        std::thread::sleep(Duration::from_millis(250));
        if let Ok(Some(status)) = child.try_wait() {
            append_log(&args.log, &format!("core exited: {status}"));
            std::thread::sleep(Duration::from_millis(200));
            std::process::exit(5);
        }
        if args.stop.exists() || !platform::process_alive(args.parent) {
            break;
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&args.stop);
}

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::{Command, Stdio};

    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn detach_console() {
        unsafe {
            windows_sys::Win32::System::Console::FreeConsole();
        }
    }

    pub fn hide_window(command: &mut Command) {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    pub fn process_alive(pid: u32) -> bool {
        unsafe {
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if handle.is_null() {
                return false;
            }
            let alive = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
            CloseHandle(handle);
            alive
        }
    }

    fn netsh(args: &[&str]) {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let _ = Command::new(Path::new(&root).join("System32").join("netsh.exe"))
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    pub fn allow_through_firewall(core: &Path) {
        let raw = core.to_string_lossy();
        let program = format!("program={}", raw.strip_prefix(r"\\?\").unwrap_or(&raw));
        netsh(&["advfirewall", "firewall", "delete", "rule", "name=all", &program]);
        for direction in ["in", "out"] {
            netsh(&[
                "advfirewall",
                "firewall",
                "add",
                "rule",
                "name=Prime VPN core",
                &format!("dir={direction}"),
                "action=allow",
                &program,
                "protocol=any",
                "enable=yes",
                "profile=any",
            ]);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;
    use std::process::Command;

    pub fn hide_window(_command: &mut Command) {}

    pub fn detach_console() {}

    pub fn process_alive(pid: u32) -> bool {
        Path::new(&format!("/proc/{pid}")).exists()
    }

    pub fn allow_through_firewall(_core: &Path) {}
}
