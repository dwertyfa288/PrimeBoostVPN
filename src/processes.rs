use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

#[derive(Serialize)]
pub struct ProcessEntry {
    pub name: String,
    pub path: String,
}

pub fn running() -> Vec<ProcessEntry> {
    let mut system = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new().with_exe(sysinfo::UpdateKind::Always)),
    );
    system.refresh_processes(ProcessesToUpdate::All);

    let mut entries: Vec<ProcessEntry> = system
        .processes()
        .values()
        .filter_map(|process| {
            let exe = process.exe()?;
            let name = exe.file_name()?.to_string_lossy().into_owned();
            Some(ProcessEntry {
                name,
                path: exe.to_string_lossy().into_owned(),
            })
        })
        .filter(|entry| !entry.name.is_empty() && !is_system_path(&entry.path))
        .collect();

    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    entries
}

fn is_system_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains(r"\windows\system32\")
        || lower.contains(r"\windows\syswow64\")
        || lower.starts_with("/usr/lib/systemd")
        || lower.starts_with("/usr/libexec")
}
