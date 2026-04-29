use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_SERIAL_READ_TIMEOUT: Duration = Duration::from_millis(100);

pub fn open_available_port(
    requested: &Path,
    baud: u32,
) -> io::Result<(PathBuf, Box<dyn serialport::SerialPort>)> {
    open_available_port_with_timeout(requested, baud, DEFAULT_SERIAL_READ_TIMEOUT)
}

pub fn open_available_port_with_timeout(
    requested: &Path,
    baud: u32,
    read_timeout: Duration,
) -> io::Result<(PathBuf, Box<dyn serialport::SerialPort>)> {
    let mut last_err = None;

    for candidate in interactive::port_candidates(requested) {
        match open_serial_port(&candidate, baud, read_timeout) {
            Ok(port) => return Ok((candidate, port)),
            Err(err) => last_err = Some(err),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("no candidate ports found for {}", requested.display()),
        )
    }))
}

fn open_serial_port(
    path: &Path,
    baud: u32,
    read_timeout: Duration,
) -> io::Result<Box<dyn serialport::SerialPort>> {
    let path = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "serial path is not UTF-8"))?;
    serialport::new(path, baud)
        .timeout(read_timeout)
        .open()
        .map_err(|err| io::Error::other(err.to_string()))
}
