use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractiveBackendMode {
    Auto,
    Compat,
    Mio,
}

impl InteractiveBackendMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "compat" => Some(Self::Compat),
            "mio" => Some(Self::Mio),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum InteractiveEvent {
    SerialBytes(Vec<u8>),
    SerialEof,
    SerialError(io::Error),
    Terminal(Event),
    Timeout,
}

enum SerialReadEvent {
    Bytes(Vec<u8>),
    Eof,
    Error(io::Error),
}

pub struct ConnectedInteractiveBackend {
    label: String,
    inner: ConnectedInteractiveBackendInner,
}

enum ConnectedInteractiveBackendInner {
    Compat(CompatBackend),
    #[cfg(unix)]
    Mio(UnixMioBackend),
}

impl ConnectedInteractiveBackend {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn next_event(&mut self, deadline: Option<Instant>) -> io::Result<InteractiveEvent> {
        match &mut self.inner {
            ConnectedInteractiveBackendInner::Compat(backend) => backend.next_event(deadline),
            #[cfg(unix)]
            ConnectedInteractiveBackendInner::Mio(backend) => backend.next_event(deadline),
        }
    }
}

impl Write for ConnectedInteractiveBackend {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.inner {
            ConnectedInteractiveBackendInner::Compat(backend) => backend.write(buf),
            #[cfg(unix)]
            ConnectedInteractiveBackendInner::Mio(backend) => backend.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.inner {
            ConnectedInteractiveBackendInner::Compat(backend) => backend.flush(),
            #[cfg(unix)]
            ConnectedInteractiveBackendInner::Mio(backend) => backend.flush(),
        }
    }
}

pub fn open_interactive_backend(
    requested: &Path,
    baud: u32,
    mode: InteractiveBackendMode,
    read_timeout: Duration,
) -> io::Result<(PathBuf, ConnectedInteractiveBackend)> {
    let mut last_err = None;

    for candidate in super::port_candidates(requested) {
        match open_candidate(&candidate, baud, mode, read_timeout) {
            Ok(backend) => return Ok((candidate, backend)),
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

fn open_candidate(
    path: &Path,
    baud: u32,
    mode: InteractiveBackendMode,
    read_timeout: Duration,
) -> io::Result<ConnectedInteractiveBackend> {
    match mode {
        InteractiveBackendMode::Auto => open_auto_backend(path, baud, read_timeout),
        InteractiveBackendMode::Compat => open_compat_backend(path, baud, read_timeout),
        InteractiveBackendMode::Mio => open_mio_backend(path, baud, read_timeout),
    }
}

fn open_auto_backend(
    path: &Path,
    baud: u32,
    read_timeout: Duration,
) -> io::Result<ConnectedInteractiveBackend> {
    #[cfg(unix)]
    {
        match open_mio_backend(path, baud, read_timeout) {
            Ok(backend) => return Ok(backend),
            Err(mio_err) => match open_compat_backend(path, baud, read_timeout) {
                Ok(mut backend) => {
                    backend.label = format!("compat (mio fallback: {mio_err})");
                    return Ok(backend);
                }
                Err(compat_err) => {
                    return Err(io::Error::new(
                        compat_err.kind(),
                        format!("mio failed: {mio_err}; compat failed: {compat_err}"),
                    ));
                }
            },
        }
    }

    #[cfg(not(unix))]
    {
        open_compat_backend(path, baud, read_timeout)
    }
}

fn open_compat_backend(
    path: &Path,
    baud: u32,
    read_timeout: Duration,
) -> io::Result<ConnectedInteractiveBackend> {
    let path_text = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "serial path is not UTF-8"))?;
    let write_port = serialport::new(path_text, baud)
        .timeout(read_timeout)
        .open()
        .map_err(|err| io::Error::other(err.to_string()))?;
    let read_port = write_port
        .try_clone()
        .map_err(|err| io::Error::other(err.to_string()))?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || read_serial_thread(read_port, tx));

    Ok(ConnectedInteractiveBackend {
        label: "compat".to_string(),
        inner: ConnectedInteractiveBackendInner::Compat(CompatBackend { write_port, rx }),
    })
}

fn read_serial_thread(
    mut port: Box<dyn serialport::SerialPort>,
    tx: mpsc::Sender<SerialReadEvent>,
) {
    let mut buf = [0u8; 4096];
    loop {
        match port.read(&mut buf) {
            Ok(0) => {
                let _ = tx.send(SerialReadEvent::Eof);
                break;
            }
            Ok(read_len) => {
                if tx
                    .send(SerialReadEvent::Bytes(buf[..read_len].to_vec()))
                    .is_err()
                {
                    break;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::TimedOut => {}
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => {
                let _ = tx.send(SerialReadEvent::Error(err));
                break;
            }
        }
    }
}

struct CompatBackend {
    write_port: Box<dyn serialport::SerialPort>,
    rx: mpsc::Receiver<SerialReadEvent>,
}

impl CompatBackend {
    fn next_event(&mut self, deadline: Option<Instant>) -> io::Result<InteractiveEvent> {
        loop {
            if let Some(event) = drain_terminal_event()? {
                return Ok(InteractiveEvent::Terminal(event));
            }
            match self.rx.try_recv() {
                Ok(event) => return Ok(map_serial_event(event)),
                Err(mpsc::TryRecvError::Disconnected) => return Ok(InteractiveEvent::SerialEof),
                Err(mpsc::TryRecvError::Empty) => {}
            }

            let Some(wait_for) = compat_wait_duration(deadline) else {
                return Ok(InteractiveEvent::Timeout);
            };
            match self.rx.recv_timeout(wait_for) {
                Ok(event) => return Ok(map_serial_event(event)),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok(InteractiveEvent::SerialEof);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        return Ok(InteractiveEvent::Timeout);
                    }
                }
            }
        }
    }
}

impl Write for CompatBackend {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_port.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.write_port.flush()
    }
}

fn compat_wait_duration(deadline: Option<Instant>) -> Option<Duration> {
    const COMPAT_POLL_INTERVAL: Duration = Duration::from_millis(10);
    match deadline {
        Some(deadline) => {
            let now = Instant::now();
            if now >= deadline {
                None
            } else {
                Some(
                    deadline
                        .saturating_duration_since(now)
                        .min(COMPAT_POLL_INTERVAL),
                )
            }
        }
        None => Some(COMPAT_POLL_INTERVAL),
    }
}

fn map_serial_event(event: SerialReadEvent) -> InteractiveEvent {
    match event {
        SerialReadEvent::Bytes(bytes) => InteractiveEvent::SerialBytes(bytes),
        SerialReadEvent::Eof => InteractiveEvent::SerialEof,
        SerialReadEvent::Error(err) => InteractiveEvent::SerialError(err),
    }
}

pub fn wait_terminal_event(deadline: Option<Instant>) -> io::Result<InteractiveEvent> {
    if let Some(event) = drain_terminal_event()? {
        return Ok(InteractiveEvent::Terminal(event));
    }

    let Some(timeout) = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()))
    else {
        return Ok(InteractiveEvent::Timeout);
    };
    if timeout.is_zero() {
        return Ok(InteractiveEvent::Timeout);
    }
    if retry_interrupted(|| event::poll(timeout))? {
        Ok(InteractiveEvent::Terminal(retry_interrupted(event::read)?))
    } else {
        Ok(InteractiveEvent::Timeout)
    }
}

pub(crate) fn drain_terminal_event() -> io::Result<Option<Event>> {
    if retry_interrupted(|| event::poll(Duration::ZERO))? {
        Ok(Some(retry_interrupted(event::read)?))
    } else {
        Ok(None)
    }
}

pub(crate) fn retry_interrupted<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match op() {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}

#[cfg(unix)]
fn open_mio_backend(
    path: &Path,
    baud: u32,
    read_timeout: Duration,
) -> io::Result<ConnectedInteractiveBackend> {
    Ok(ConnectedInteractiveBackend {
        label: "mio".to_string(),
        inner: ConnectedInteractiveBackendInner::Mio(UnixMioBackend::open(
            path,
            baud,
            read_timeout,
        )?),
    })
}

#[cfg(not(unix))]
fn open_mio_backend(
    _path: &Path,
    _baud: u32,
    _read_timeout: Duration,
) -> io::Result<ConnectedInteractiveBackend> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "mio backend is only available on Unix",
    ))
}

#[cfg(unix)]
mod unix_mio {
    use std::fs::File;
    use std::io::{self, IsTerminal, Read, Write};
    use std::os::fd::AsRawFd;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use crossterm::event::Event;
    use mio::unix::SourceFd;
    use mio::{Events, Interest, Poll, Token};
    use serialport::SerialPort;
    use signal_hook::consts::SIGWINCH;
    use signal_hook_mio::v1_0::Signals;

    use super::{drain_terminal_event, InteractiveEvent};

    const SERIAL_TOKEN: Token = Token(0);
    const TERMINAL_TOKEN: Token = Token(1);
    const SIGNAL_TOKEN: Token = Token(2);

    pub(super) struct UnixMioBackend {
        port: serialport::TTYPort,
        poll: Poll,
        events: Events,
        _terminal_file: Option<File>,
        signals: Signals,
    }

    impl UnixMioBackend {
        pub(super) fn open(path: &Path, baud: u32, read_timeout: Duration) -> io::Result<Self> {
            let path_text = path.to_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "serial path is not UTF-8")
            })?;
            let mut port = serialport::new(path_text, baud)
                .timeout(read_timeout)
                .open_native()
                .map_err(|err| io::Error::other(err.to_string()))?;
            port.set_timeout(Duration::from_millis(1))
                .map_err(|err| io::Error::other(err.to_string()))?;

            let poll = Poll::new()?;
            let registry = poll.registry();

            let serial_fd = port.as_raw_fd();
            let mut serial_source = SourceFd(&serial_fd);
            registry.register(&mut serial_source, SERIAL_TOKEN, Interest::READABLE)?;

            let (terminal_fd, terminal_file) = terminal_fd()?;
            let mut terminal_source = SourceFd(&terminal_fd);
            registry.register(&mut terminal_source, TERMINAL_TOKEN, Interest::READABLE)?;

            let mut signals = Signals::new([SIGWINCH])?;
            registry.register(&mut signals, SIGNAL_TOKEN, Interest::READABLE)?;

            Ok(Self {
                port,
                poll,
                events: Events::with_capacity(8),
                _terminal_file: terminal_file,
                signals,
            })
        }

        pub(super) fn next_event(
            &mut self,
            deadline: Option<Instant>,
        ) -> io::Result<InteractiveEvent> {
            if let Some(event) = drain_terminal_event()? {
                return Ok(InteractiveEvent::Terminal(event));
            }

            let timeout =
                deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
            super::retry_interrupted(|| self.poll.poll(&mut self.events, timeout))?;
            if self.events.is_empty() {
                return Ok(InteractiveEvent::Timeout);
            }

            for token in self.events.iter().map(|event| event.token()) {
                match token {
                    SERIAL_TOKEN => {
                        let mut buf = [0u8; 4096];
                        match self.port.read(&mut buf) {
                            Ok(0) => return Ok(InteractiveEvent::SerialEof),
                            Ok(read_len) => {
                                return Ok(InteractiveEvent::SerialBytes(buf[..read_len].to_vec()));
                            }
                            Err(err) if err.kind() == io::ErrorKind::TimedOut => {}
                            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
                            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                            Err(err) => return Ok(InteractiveEvent::SerialError(err)),
                        }
                    }
                    TERMINAL_TOKEN => {
                        if let Some(event) = drain_terminal_event()? {
                            return Ok(InteractiveEvent::Terminal(event));
                        }
                    }
                    SIGNAL_TOKEN => {
                        if self.signals.pending().any(|signal| signal == SIGWINCH) {
                            let (cols, rows) = super::retry_interrupted(crossterm::terminal::size)?;
                            return Ok(InteractiveEvent::Terminal(Event::Resize(cols, rows)));
                        }
                    }
                    _ => {}
                }
            }

            Ok(InteractiveEvent::Timeout)
        }
    }

    impl Write for UnixMioBackend {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.port.write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.port.flush()
        }
    }

    fn terminal_fd() -> io::Result<(i32, Option<File>)> {
        let stdin = io::stdin();
        if stdin.is_terminal() {
            return Ok((stdin.as_raw_fd(), None));
        }

        let file = File::options().read(true).write(true).open("/dev/tty")?;
        let fd = file.as_raw_fd();
        Ok((fd, Some(file)))
    }
}

#[cfg(unix)]
use unix_mio::UnixMioBackend;

#[cfg(test)]
mod tests {
    use super::retry_interrupted;
    use std::cell::Cell;
    use std::io;

    #[test]
    fn retry_interrupted_retries_until_success() {
        let attempts = Cell::new(0);

        let value = retry_interrupted(|| {
            let next = attempts.get() + 1;
            attempts.set(next);
            if next < 3 {
                Err(io::Error::from(io::ErrorKind::Interrupted))
            } else {
                Ok("ready")
            }
        })
        .expect("interrupted operations should be retried");

        assert_eq!(value, "ready");
        assert_eq!(attempts.get(), 3);
    }

    #[test]
    fn retry_interrupted_preserves_non_interrupted_error() {
        let err = retry_interrupted(|| -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::WouldBlock))
        })
        .expect_err("non-interrupted errors should still propagate");

        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
    }
}
