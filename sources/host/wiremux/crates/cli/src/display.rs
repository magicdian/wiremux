use std::collections::HashMap;
use std::io::{self, Write};

use host_session::{display_channel_name, DeviceManifest, MuxEnvelope};

pub struct DisplayOutput<W: Write> {
    pub out: W,
    pub channel_filter: Option<u32>,
    line_open: bool,
    line_channel: Option<u32>,
    channel_names: HashMap<u32, String>,
}

impl<W: Write> DisplayOutput<W> {
    pub fn new(out: W, channel_filter: Option<u32>) -> Self {
        Self {
            out,
            channel_filter,
            line_open: false,
            line_channel: None,
            channel_names: HashMap::new(),
        }
    }

    pub fn write_terminal(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.channel_filter.is_some() {
            return Ok(());
        }

        self.out.write_all(bytes)?;
        self.update_line_state(bytes, None);
        Ok(())
    }

    pub fn write_record(&mut self, envelope: &MuxEnvelope) -> io::Result<()> {
        if self
            .channel_filter
            .is_some_and(|channel| channel != envelope.channel_id)
        {
            return Ok(());
        }

        if self.channel_filter.is_some() {
            self.out.write_all(&envelope.payload)?;
            return Ok(());
        }

        self.prepare_unfiltered_record(envelope.channel_id)?;
        write!(self.out, "{}> ", self.channel_prefix(envelope.channel_id))?;
        self.line_open = true;
        self.line_channel = Some(envelope.channel_id);
        self.out.write_all(&envelope.payload)?;
        self.update_line_state(&envelope.payload, Some(envelope.channel_id));
        Ok(())
    }

    pub fn update_manifest(&mut self, manifest: &DeviceManifest) {
        self.channel_names.clear();
        for channel in &manifest.channels {
            if let Some(name) = display_channel_name(&channel.name) {
                self.channel_names.insert(channel.channel_id, name);
            }
        }
    }

    pub fn write_marker_line(&mut self, message: &str) -> io::Result<()> {
        if self.line_open {
            self.out.write_all(b"\n")?;
        }
        writeln!(self.out, "wiremux> {message}")?;
        self.line_open = false;
        self.line_channel = None;
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }

    fn channel_prefix(&self, channel_id: u32) -> String {
        match self.channel_names.get(&channel_id) {
            Some(name) => format!("ch{channel_id}({name})"),
            None => format!("ch{channel_id}"),
        }
    }

    fn prepare_unfiltered_record(&mut self, channel_id: u32) -> io::Result<()> {
        if !self.line_open || self.line_channel == Some(channel_id) {
            return Ok(());
        }

        match self.line_channel {
            Some(previous_channel) => {
                self.out.write_all(b"\n")?;
                writeln!(
                    self.out,
                    "wiremux> continued after partial ch{} line",
                    previous_channel
                )?;
            }
            None => {
                self.out.write_all(b"\n")?;
            }
        }
        self.line_open = false;
        self.line_channel = None;
        Ok(())
    }

    fn update_line_state(&mut self, bytes: &[u8], channel: Option<u32>) {
        if bytes.is_empty() {
            return;
        }

        if bytes
            .last()
            .is_some_and(|byte| *byte == b'\n' || *byte == b'\r')
        {
            self.line_open = false;
            self.line_channel = None;
        } else {
            self.line_open = true;
            self.line_channel = channel;
        }
    }
}
