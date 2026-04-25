const IEEE_POLY_REVERSED: u32 = 0xedb8_8320;

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;

    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (IEEE_POLY_REVERSED & mask);
        }
    }

    !crc
}

#[cfg(test)]
mod tests {
    use super::crc32;

    #[test]
    fn crc32_matches_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn crc32_empty_input() {
        assert_eq!(crc32(b""), 0);
    }
}
