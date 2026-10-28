use std::process::Command;

pub fn open(url: &str) -> Result<(), String> {
    let site = format!("{}/", crate::endpoints::site_url());
    if !url.starts_with(&site) && !url.starts_with("https://t.me/") {
        return Err("Адрес не разрешён".into());
    }
    launch(url).map_err(|error| format!("Не удалось открыть браузер: {error}"))
}

pub fn open_payment(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") || url.chars().any(char::is_whitespace) {
        return Err("Некорректная ссылка оплаты".into());
    }
    launch(url).map_err(|error| format!("Не удалось открыть браузер: {error}"))
}

#[cfg(windows)]
fn launch(url: &str) -> std::io::Result<()> {
    Command::new(crate::paths::system32("rundll32.exe"))
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map(|_| ())
}

#[cfg(not(windows))]
fn launch(url: &str) -> std::io::Result<()> {
    Command::new("xdg-open").arg(url).spawn().map(|_| ())
}
