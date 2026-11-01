//! Bounded raster container checks; final decoding belongs to the WebView.
const LIMIT: usize = 1024 * 1024;
fn dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return Err("icon_dimensions_unsupported".into());
    }
    Ok(())
}
fn be32(bytes: &[u8]) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        bytes.try_into().map_err(|_| "icon_invalid_image")?,
    ))
}
fn le24(bytes: &[u8]) -> u32 {
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
}

pub(super) fn mime(bytes: &[u8]) -> Result<&'static str, String> {
    if bytes.len() > LIMIT {
        return Err("icon_too_large".into());
    }
    if bytes.len() >= 45 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        if bytes.get(8..16) != Some(b"\0\0\0\rIHDR".as_slice()) {
            return Err("icon_invalid_image".into());
        }
        dimensions(be32(&bytes[16..20])?, be32(&bytes[20..24])?)?;
        let mut offset = 8usize;
        let mut image_data = false;
        while offset + 12 <= bytes.len() {
            let length = be32(&bytes[offset..offset + 4])? as usize;
            let end = offset
                .checked_add(length)
                .and_then(|n| n.checked_add(12))
                .ok_or("icon_invalid_image")?;
            if end > bytes.len() {
                return Err("icon_invalid_image".into());
            }
            let kind = &bytes[offset + 4..offset + 8];
            if kind == b"acTL" {
                return Err("icon_animated_unsupported".into());
            }
            image_data |= kind == b"IDAT";
            if kind == b"IEND" {
                return if image_data && length == 0 && end == bytes.len() {
                    Ok("image/png")
                } else {
                    Err("icon_invalid_image".into())
                };
            }
            offset = end;
        }
        return Err("icon_invalid_image".into());
    }
    if bytes.len() >= 12 && bytes.starts_with(b"\xff\xd8") && bytes.ends_with(b"\xff\xd9") {
        let mut offset = 2;
        while offset + 4 <= bytes.len() {
            if bytes[offset] != 0xff {
                return Err("icon_invalid_image".into());
            }
            let marker = bytes[offset + 1];
            if marker == 0xff {
                offset += 1;
                continue;
            }
            let length = usize::from(u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]));
            if length < 2 || offset + 2 + length > bytes.len() {
                return Err("icon_invalid_image".into());
            }
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf)
                && length >= 8
            {
                dimensions(
                    u32::from(u16::from_be_bytes([bytes[offset + 7], bytes[offset + 8]])),
                    u32::from(u16::from_be_bytes([bytes[offset + 5], bytes[offset + 6]])),
                )?;
                return Ok("image/jpeg");
            }
            if marker == 0xda {
                break;
            }
            offset += 2 + length;
        }
        return Err("icon_invalid_image".into());
    }
    if bytes.len() >= 30 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        let size = u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| "icon_invalid_image")?);
        if usize::try_from(size).ok().and_then(|n| n.checked_add(8)) != Some(bytes.len()) {
            return Err("icon_invalid_image".into());
        }
        let (width, height) = match &bytes[12..16] {
            b"VP8X" if bytes[20] & 2 == 0 => (le24(&bytes[24..27]) + 1, le24(&bytes[27..30]) + 1),
            b"VP8L" if bytes[20] == 0x2f => {
                let bits =
                    u32::from_le_bytes(bytes[21..25].try_into().map_err(|_| "icon_invalid_image")?);
                ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
            }
            b"VP8 " if bytes.get(23..26) == Some(b"\x9d\x01\x2a".as_slice()) => (
                u32::from(u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3fff),
                u32::from(u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3fff),
            ),
            _ => return Err("icon_invalid_image".into()),
        };
        dimensions(width, height)?;
        return Ok("image/webp");
    }
    if bytes.len() >= 14 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        dimensions(
            u32::from(u16::from_le_bytes([bytes[6], bytes[7]])),
            u32::from(u16::from_le_bytes([bytes[8], bytes[9]])),
        )?;
        let table = |packed: u8| {
            if packed & 0x80 != 0 {
                3usize * (1usize << ((packed & 7) + 1))
            } else {
                0
            }
        };
        let mut offset = 13 + table(bytes[10]);
        let mut frames = 0;
        loop {
            let marker = *bytes.get(offset).ok_or("icon_invalid_image")?;
            match marker {
                0x3b => {
                    return if frames == 1 && offset + 1 == bytes.len() {
                        Ok("image/gif")
                    } else {
                        Err("icon_invalid_image".into())
                    };
                }
                0x21 => {
                    offset += 2;
                }
                0x2c => {
                    frames += 1;
                    if frames > 1 {
                        return Err("icon_animated_unsupported".into());
                    }
                    let descriptor = bytes.get(offset..offset + 10).ok_or("icon_invalid_image")?;
                    dimensions(
                        u32::from(u16::from_le_bytes([descriptor[5], descriptor[6]])),
                        u32::from(u16::from_le_bytes([descriptor[7], descriptor[8]])),
                    )?;
                    offset += 11 + table(descriptor[9]);
                }
                _ => return Err("icon_invalid_image".into()),
            }
            loop {
                let length = usize::from(*bytes.get(offset).ok_or("icon_invalid_image")?);
                offset += 1;
                if length == 0 {
                    break;
                }
                offset += length;
                if offset > bytes.len() {
                    return Err("icon_invalid_image".into());
                }
            }
        }
    }
    Err("icon_format_unsupported".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    #[test]
    fn bounded_raster_not_extension_or_svg() -> Result<(), String> {
        let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aC1sAAAAASUVORK5CYII=").map_err(|_| "fixture decode failed")?;
        assert_eq!(mime(&png)?, "image/png");
        assert!(mime(&png[..png.len() - 1]).is_err());
        assert!(mime(b"<svg onload='alert(1)'/>").is_err());
        assert!(mime(&vec![0; LIMIT + 1]).is_err());
        let mut huge = png;
        huge[16..20].copy_from_slice(&1025u32.to_be_bytes());
        assert!(mime(&huge).is_err());
        Ok(())
    }
}
