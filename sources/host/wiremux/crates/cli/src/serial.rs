use std::io;
use std::path::PathBuf;
use std::time::Duration;

use interactive::SerialProfile;

pub const DEFAULT_SERIAL_READ_TIMEOUT: Duration = Duration::from_millis(100);

pub fn open_available_port(
    profile: &SerialProfile,
) -> io::Result<(PathBuf, Box<dyn serialport::SerialPort>)> {
    open_available_port_with_timeout(profile, DEFAULT_SERIAL_READ_TIMEOUT)
}

pub fn open_available_port_with_timeout(
    profile: &SerialProfile,
    read_timeout: Duration,
) -> io::Result<(PathBuf, Box<dyn serialport::SerialPort>)> {
    let mut last_err = None;

    for candidate in interactive::port_candidates(&profile.port) {
        match open_serial_port(&candidate, profile, read_timeout) {
            Ok(port) => return Ok((candidate, port)),
            Err(err) => last_err = Some(err),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("no candidate ports found for {}", profile.port.display()),
        )
    }))
}

fn open_serial_port(
    path: &std::path::Path,
    profile: &SerialProfile,
    read_timeout: Duration,
) -> io::Result<Box<dyn serialport::SerialPort>> {
    let path = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "serial path is not UTF-8"))?;
    profile
        .apply_to_builder(serialport::new(path, profile.baud).timeout(read_timeout))?
        .open()
        .map_err(|err| io::Error::other(err.to_string()))
}
