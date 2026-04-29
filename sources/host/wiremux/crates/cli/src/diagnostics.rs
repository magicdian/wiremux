use std::env;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn create_diagnostics_file(requested_port: &Path) -> io::Result<(PathBuf, File)> {
    let dir = env::temp_dir().join("wiremux");
    fs::create_dir_all(&dir)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let filename = format!(
        "wiremux-{}-{:06}-{}.log",
        now.as_secs(),
        now.subsec_micros(),
        sanitize_port_for_filename(requested_port)
    );
    let path = dir.join(filename);
    let file = File::create(&path)?;
    Ok((path, file))
}

pub fn sanitize_port_for_filename(port: &Path) -> String {
    let value = port.to_string_lossy();
    let mut sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    while sanitized.starts_with('_') {
        sanitized.remove(0);
    }
    while sanitized.ends_with('_') {
        sanitized.pop();
    }

    if sanitized.is_empty() {
        "port".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_port_for_filename;
    use std::path::PathBuf;

    #[test]
    fn sanitizes_port_for_diagnostics_filename() {
        assert_eq!(
            sanitize_port_for_filename(&PathBuf::from("/dev/cu.usbmodem2101")),
            "dev_cu.usbmodem2101"
        );
        assert_eq!(sanitize_port_for_filename(&PathBuf::from("/")), "port");
    }
}
