use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use crate::paths;
use crate::privileges;
use crate::xray::{BridgeLaunch, XrayBridge};

const PID_FILE: &str = "core.pid";
const XRAY_PID_FILE: &str = "xray.pid";
#[cfg(windows)]
const CORE_IMAGE: &str = "prime-vpn-core.exe";
#[cfg(windows)]
const XRAY_IMAGE: &str = "prime-vpn-xray.exe";
#[cfg(not(windows))]
const CORE_IMAGE: &str = "prime-vpn-core";
#[cfg(not(windows))]
const XRAY_IMAGE: &str = "prime-vpn-xray";
const LOG_LIMIT: usize = 40;

enum Running {
    Direct(Child),
    #[cfg(windows)]
    Elevated(elevated::Session),
}

#[derive(Default)]
pub struct Core {
    running: Mutex<Option<Running>>,
    bridge: Mutex<Option<XrayBridge>>,
    log: Arc<Mutex<VecDeque<String>>>,
}

impl Core {
    pub fn start(&self, config_json: &str, needs_tun: bool, bridge: Option<&BridgeLaunch>) -> Result<(), String> {
        self.stop();
        kill_orphan(PID_FILE, CORE_IMAGE);
        kill_orphan(XRAY_PID_FILE, XRAY_IMAGE);
        self.log.lock().clear();

        let binary = crate::core_assets::ensure()?;
        ensure_executable(&binary);
        let result = self.start_bridge(bridge).and_then(|_| self.start_core(&binary, config_json, needs_tun));
        if result.is_err() {
            self.stop();
        }
        result
    }

    fn start_bridge(&self, bridge: Option<&BridgeLaunch>) -> Result<(), String> {
        let Some(launch) = bridge else {
            return Ok(());
        };
        let binary = crate::core_assets::xray_path();
        ensure_executable(&binary);
        let bridge = XrayBridge::start(&binary, launch, self.log.clone(), LOG_LIMIT)?;
        let _ = std::fs::write(paths::data_dir().join(XRAY_PID_FILE), bridge.pid().to_string());
        *self.bridge.lock() = Some(bridge);
        Ok(())
    }

    fn start_core(&self, binary: &std::path::Path, config_json: &str, needs_tun: bool) -> Result<(), String> {

        if needs_tun && !privileges::process_elevated() {
            #[cfg(windows)]
            {
                let session = elevated::start(binary, config_json)?;
                *self.running.lock() = Some(Running::Elevated(session));
                return self.wait_startup(Duration::from_millis(1500));
            }
            #[cfg(not(windows))]
            privileges::ensure_tun_capability(binary)?;
        }

        let child = self.spawn_direct(binary, config_json)?;
        let _ = std::fs::write(paths::data_dir().join(PID_FILE), child.id().to_string());
        *self.running.lock() = Some(Running::Direct(child));
        self.wait_startup(Duration::from_millis(800))
    }

    fn spawn_direct(&self, binary: &std::path::Path, config_json: &str) -> Result<Child, String> {
        let mut command = Command::new(binary);
        command
            .args(["run", "-c", "stdin"])
            .current_dir(paths::core_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        hide_window(&mut command);

        let mut child = command
            .spawn()
            .map_err(|error| format!("Не удалось запустить ядро VPN: {error}"))?;

        let written = child
            .stdin
            .take()
            .ok_or_else(|| "Нет доступа к stdin ядра".to_owned())
            .and_then(|mut stdin| {
                stdin
                    .write_all(config_json.as_bytes())
                    .and_then(|_| stdin.flush())
                    .map_err(|error| format!("Не удалось передать конфигурацию: {error}"))
            });
        if let Err(error) = written {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }

        if let Some(stderr) = child.stderr.take() {
            let log = self.log.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut log = log.lock();
                    log.push_back(line);
                    if log.len() > LOG_LIMIT {
                        log.pop_front();
                    }
                }
            });
        }
        Ok(child)
    }

    fn wait_startup(&self, delay: Duration) -> Result<(), String> {
        std::thread::sleep(delay);
        if self.is_running() {
            return Ok(());
        }
        let tail = self.last_errors();
        Err(if tail.is_empty() {
            "Ядро VPN завершилось при запуске".into()
        } else {
            format!("Ядро VPN завершилось при запуске: {tail}")
        })
    }

    pub fn stop(&self) {
        match self.running.lock().take() {
            Some(Running::Direct(mut child)) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            #[cfg(windows)]
            Some(Running::Elevated(session)) => session.stop(),
            None => {}
        }
        let _ = std::fs::remove_file(paths::data_dir().join(PID_FILE));
        *self.bridge.lock() = None;
        let _ = std::fs::remove_file(paths::data_dir().join(XRAY_PID_FILE));
    }

    pub fn is_running(&self) -> bool {
        if let Some(bridge) = self.bridge.lock().as_mut() {
            if !bridge.is_running() {
                return false;
            }
        }
        let mut guard = self.running.lock();
        match guard.as_mut() {
            Some(Running::Direct(child)) => matches!(child.try_wait(), Ok(None)),
            #[cfg(windows)]
            Some(Running::Elevated(session)) => session.is_alive(),
            None => false,
        }
    }

    pub fn last_errors(&self) -> String {
        #[cfg(windows)]
        if let Some(Running::Elevated(session)) = self.running.lock().as_ref() {
            return session.log_tail(3);
        }
        #[cfg(windows)]
        if self.log.lock().is_empty() {
            let tail = elevated::read_log_tail(3);
            if !tail.is_empty() {
                return tail;
            }
        }
        let log = self.log.lock();
        log.iter()
            .rev()
            .take(3)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        self.stop();
    }
}

fn kill_orphan(pid_file: &str, image: &str) {
    let path = paths::data_dir().join(pid_file);
    let Ok(pid) = std::fs::read_to_string(&path) else {
        return;
    };
    let pid = pid.trim();
    if pid.parse::<u32>().is_ok() {
        kill_pid(pid, image);
    }
    let _ = std::fs::remove_file(&path);
}

#[cfg(windows)]
fn kill_pid(pid: &str, image: &str) {
    let mut command = Command::new(paths::system32("taskkill.exe"));
    command.args([
        "/F",
        "/FI",
        &format!("PID eq {pid}"),
        "/FI",
        &format!("IMAGENAME eq {image}"),
    ]);
    hide_window(&mut command);
    let _ = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
}

#[cfg(not(windows))]
fn kill_pid(pid: &str, image: &str) {
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
    if comm.trim().starts_with(image) {
        let _ = Command::new("kill").arg(pid).status();
    }
}

#[cfg(unix)]
fn ensure_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) {
        let mut permissions = metadata.permissions();
        if permissions.mode() & 0o111 == 0 {
            permissions.set_mode(0o755);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
}

#[cfg(not(unix))]
fn ensure_executable(_path: &std::path::Path) {}

#[cfg(windows)]
pub fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_window(_command: &mut Command) {}

#[cfg(windows)]
mod elevated {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, GetLastError, HANDLE, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SEE_MASK_NO_CONSOLE, SHELLEXECUTEINFOW, ShellExecuteExW,
    };

    use crate::paths;

    const CONFIG_FILE: &str = "core-config.json";
    const STOP_FILE: &str = "core.stop";
    const LOG_FILE: &str = "core.log";
    const SW_HIDE: i32 = 0;

    pub struct Session {
        handle: HANDLE,
        stop_file: PathBuf,
    }

    unsafe impl Send for Session {}

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn quote(path: &Path) -> String {
        format!("\"{}\"", path.display())
    }

    fn runner_path() -> Result<PathBuf, String> {
        std::env::current_exe().map_err(|error| format!("Не найден помощник запуска: {error}"))
    }

    pub fn start(core: &Path, config_json: &str) -> Result<Session, String> {
        let data = paths::data_dir();
        let config = data.join(CONFIG_FILE);
        let stop_file = data.join(STOP_FILE);
        let log = data.join(LOG_FILE);
        let _ = std::fs::remove_file(&stop_file);
        std::fs::write(&config, config_json)
            .map_err(|error| format!("Не удалось подготовить конфигурацию: {error}"))?;

        let runner = runner_path()?;
        let parameters = format!(
            "{} --core {} --config {} --stop {} --log {} --parent {}",
            crate::runner::FLAG,
            quote(core),
            quote(&config),
            quote(&stop_file),
            quote(&log),
            std::process::id()
        );
        let verb = wide("runas");
        let file = wide(&runner.to_string_lossy());
        let params = wide(&parameters);
        let directory = wide(&paths::core_dir().to_string_lossy());

        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NO_CONSOLE;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.lpDirectory = directory.as_ptr();
        info.nShow = SW_HIDE;

        let ok = unsafe { ShellExecuteExW(&mut info) };
        if ok == 0 || info.hProcess.is_null() {
            let _ = std::fs::remove_file(&config);
            let code = unsafe { GetLastError() };
            return Err(if code == ERROR_CANCELLED {
                "Нужны права администратора: подтверди запрос Windows".into()
            } else {
                format!("Не удалось запустить ядро с правами администратора (код {code})")
            });
        }
        Ok(Session {
            handle: info.hProcess,
            stop_file,
        })
    }

    pub fn read_log_tail(lines: usize) -> String {
        let text = std::fs::read_to_string(paths::data_dir().join(LOG_FILE)).unwrap_or_default();
        let tail: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
        tail[tail.len().saturating_sub(lines)..].join(" | ")
    }

    impl Session {
        pub fn is_alive(&self) -> bool {
            unsafe { WaitForSingleObject(self.handle, 0) == WAIT_TIMEOUT }
        }

        pub fn log_tail(&self, lines: usize) -> String {
            read_log_tail(lines)
        }

        pub fn stop(self) {
            let _ = std::fs::write(&self.stop_file, b"stop");
            unsafe {
                WaitForSingleObject(self.handle, Duration::from_secs(5).as_millis() as u32);
            }
            let _ = std::fs::remove_file(&self.stop_file);
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}
