use crate::envelope::{read_len_delimited, read_varint, DecodeError};

pub const MANIFEST_PAYLOAD_TYPE: &str = "wiremux.v1.DeviceManifest";
pub const MANIFEST_REQUEST_PAYLOAD_TYPE: &str = "wiremux.v1.DeviceManifestRequest";

pub const INTERACTION_MODE_UNSPECIFIED: u32 = 0;
pub const INTERACTION_MODE_LINE: u32 = 1;
pub const INTERACTION_MODE_PASSTHROUGH: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelDescriptor {
    pub channel_id: u32,
    pub name: String,
    pub description: String,
    pub directions: Vec<u32>,
    pub payload_kinds: Vec<u32>,
    pub payload_types: Vec<String>,
    pub flags: u32,
    pub default_payload_kind: u32,
    pub interaction_modes: Vec<u32>,
    pub default_interaction_mode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceManifest {
    pub device_name: String,
    pub firmware_version: String,
    pub protocol_version: u32,
    pub max_channels: u32,
    pub channels: Vec<ChannelDescriptor>,
    pub native_endianness: u32,
    pub max_payload_len: u32,
    pub transport: String,
    pub feature_flags: u32,
    pub sdk_name: String,
    pub sdk_version: String,
}

pub fn encode_manifest_request() -> Vec<u8> {
    Vec::new()
}

pub fn decode_manifest(bytes: &[u8]) -> Result<DeviceManifest, DecodeError> {
    let mut cursor = 0;
    let mut manifest = DeviceManifest {
        device_name: String::new(),
        firmware_version: String::new(),
        protocol_version: 0,
        max_channels: 0,
        channels: Vec::new(),
        native_endianness: 0,
        max_payload_len: 0,
        transport: String::new(),
        feature_flags: 0,
        sdk_name: String::new(),
        sdk_version: String::new(),
    };

    while cursor < bytes.len() {
        let key = read_varint(bytes, &mut cursor)?;
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        match (field_number, wire_type) {
            (1, 2) => manifest.device_name = read_string(bytes, &mut cursor)?,
            (2, 2) => manifest.firmware_version = read_string(bytes, &mut cursor)?,
            (3, 0) => manifest.protocol_version = read_varint(bytes, &mut cursor)? as u32,
            (4, 0) => manifest.max_channels = read_varint(bytes, &mut cursor)? as u32,
            (5, 2) => {
                let channel = read_len_delimited(bytes, &mut cursor)?;
                manifest.channels.push(decode_channel(&channel)?);
            }
            (6, 0) => manifest.native_endianness = read_varint(bytes, &mut cursor)? as u32,
            (7, 0) => manifest.max_payload_len = read_varint(bytes, &mut cursor)? as u32,
            (8, 2) => manifest.transport = read_string(bytes, &mut cursor)?,
            (9, 0) => manifest.feature_flags = read_varint(bytes, &mut cursor)? as u32,
            (10, 2) => manifest.sdk_name = read_string(bytes, &mut cursor)?,
            (11, 2) => manifest.sdk_version = read_string(bytes, &mut cursor)?,
            (_, 0) => {
                let _ = read_varint(bytes, &mut cursor)?;
            }
            (_, 2) => {
                let _ = read_len_delimited(bytes, &mut cursor)?;
            }
            (_, unsupported) => return Err(DecodeError::UnsupportedWireType(unsupported)),
        }
    }

    Ok(manifest)
}

fn decode_channel(bytes: &[u8]) -> Result<ChannelDescriptor, DecodeError> {
    let mut cursor = 0;
    let mut channel = ChannelDescriptor {
        channel_id: 0,
        name: String::new(),
        description: String::new(),
        directions: Vec::new(),
        payload_kinds: Vec::new(),
        payload_types: Vec::new(),
        flags: 0,
        default_payload_kind: 0,
        interaction_modes: Vec::new(),
        default_interaction_mode: INTERACTION_MODE_UNSPECIFIED,
    };

    while cursor < bytes.len() {
        let key = read_varint(bytes, &mut cursor)?;
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        match (field_number, wire_type) {
            (1, 0) => channel.channel_id = read_varint(bytes, &mut cursor)? as u32,
            (2, 2) => channel.name = read_string(bytes, &mut cursor)?,
            (3, 2) => channel.description = read_string(bytes, &mut cursor)?,
            (4, 0) => channel
                .directions
                .push(read_varint(bytes, &mut cursor)? as u32),
            (5, 0) => channel
                .payload_kinds
                .push(read_varint(bytes, &mut cursor)? as u32),
            (6, 2) => channel.payload_types.push(read_string(bytes, &mut cursor)?),
            (7, 0) => channel.flags = read_varint(bytes, &mut cursor)? as u32,
            (8, 0) => channel.default_payload_kind = read_varint(bytes, &mut cursor)? as u32,
            (9, 0) => channel
                .interaction_modes
                .push(read_varint(bytes, &mut cursor)? as u32),
            (10, 0) => {
                channel.default_interaction_mode = read_varint(bytes, &mut cursor)? as u32;
            }
            (_, 0) => {
                let _ = read_varint(bytes, &mut cursor)?;
            }
            (_, 2) => {
                let _ = read_len_delimited(bytes, &mut cursor)?;
            }
            (_, unsupported) => return Err(DecodeError::UnsupportedWireType(unsupported)),
        }
    }

    Ok(channel)
}

fn read_string(bytes: &[u8], cursor: &mut usize) -> Result<String, DecodeError> {
    String::from_utf8(read_len_delimited(bytes, cursor)?).map_err(|_| DecodeError::InvalidUtf8)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_manifest, encode_manifest_request, INTERACTION_MODE_LINE,
        INTERACTION_MODE_PASSTHROUGH,
    };
    use crate::envelope::{write_bytes_field, write_varint_field};

    #[test]
    fn manifest_request_is_empty_message() {
        assert!(encode_manifest_request().is_empty());
    }

    #[test]
    fn decodes_manifest_with_channel_interaction_modes() {
        let mut channel = Vec::new();
        write_varint_field(&mut channel, 1, 1);
        write_bytes_field(&mut channel, 2, b"console");
        write_varint_field(&mut channel, 4, 1);
        write_varint_field(&mut channel, 4, 2);
        write_varint_field(&mut channel, 5, 1);
        write_varint_field(&mut channel, 8, 1);
        write_varint_field(&mut channel, 9, INTERACTION_MODE_LINE.into());
        write_varint_field(&mut channel, 9, INTERACTION_MODE_PASSTHROUGH.into());
        write_varint_field(&mut channel, 10, INTERACTION_MODE_LINE.into());

        let mut manifest = Vec::new();
        write_bytes_field(&mut manifest, 1, b"esp-wiremux");
        write_varint_field(&mut manifest, 3, 1);
        write_varint_field(&mut manifest, 4, 8);
        write_bytes_field(&mut manifest, 5, &channel);
        write_varint_field(&mut manifest, 7, 512);
        write_bytes_field(&mut manifest, 8, b"usb_serial_jtag");
        write_varint_field(&mut manifest, 9, 0x11);

        let decoded = decode_manifest(&manifest).expect("manifest decodes");
        assert_eq!(decoded.device_name, "esp-wiremux");
        assert_eq!(decoded.max_payload_len, 512);
        assert_eq!(decoded.channels.len(), 1);
        assert_eq!(decoded.channels[0].name, "console");
        assert_eq!(
            decoded.channels[0].interaction_modes,
            vec![INTERACTION_MODE_LINE, INTERACTION_MODE_PASSTHROUGH]
        );
        assert_eq!(
            decoded.channels[0].default_interaction_mode,
            INTERACTION_MODE_LINE
        );
    }
}
