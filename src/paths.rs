use std::path::PathBuf;

pub fn plugin_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    let dir = plugin_dir().join("data");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn core_dir() -> PathBuf {
    data_dir().join("core")
}

#[cfg(windows)]
pub fn system32(program: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("windir"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32").join(program)
}
