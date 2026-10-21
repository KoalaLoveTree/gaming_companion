//! The tray and task-switcher mark: a sheet beside a screen.
//!
//! Drawn here so both sizes stay the same picture. The desktop file points at
//! a png of it. That file is what the switcher shows.

pub(crate) const TRAY: u32 = 32;
pub(crate) const SWITCHER: u32 = 128;

const TILE: [u8; 3] = [44, 46, 50];
const SCREEN: [u8; 3] = [138, 138, 142];
const SHEET: [u8; 3] = [243, 240, 230];

/// Left, top, width, height, corner radius, as fractions of the icon.
const TILE_BOX: [f32; 5] = [0.0, 0.0, 1.0, 1.0, 0.22];
const SCREEN_BOX: [f32; 5] = [0.20, 0.24, 0.48, 0.52, 0.05];
const SHEET_BOX: [f32; 5] = [0.58, 0.34, 0.24, 0.32, 0.045];

pub(crate) fn pixels(size: u32) -> Vec<u8> {
    let size = size.max(1);
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let span = size as f32;
    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let mut pixel = over(
                [0.0; 4],
                TILE,
                coverage(round_distance(px, py, span, TILE_BOX)),
            );
            // The sheet sits behind the screen, so only its right edge shows.
            pixel = over(
                pixel,
                SHEET,
                coverage(round_distance(px, py, span, SHEET_BOX)),
            );
            pixel = over(
                pixel,
                SCREEN,
                coverage(round_distance(px, py, span, SCREEN_BOX)),
            );
            let index = ((y * size + x) * 4) as usize;
            for channel in 0..3 {
                pixels[index + channel] = (pixel[channel] * 255.0).round() as u8;
            }
            pixels[index + 3] = (pixel[3] * 255.0).round() as u8;
        }
    }
    pixels
}

pub(crate) fn png(size: u32) -> Vec<u8> {
    let size = size.max(1);
    let rgba = pixels(size);
    let mut raw = Vec::with_capacity((size as usize) * (1 + size as usize * 4));
    let row = (size * 4) as usize;
    for y in 0..size as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * row..(y + 1) * row]);
    }

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);

    let mut file = Vec::new();
    file.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    file.extend(chunk(b"IHDR", &ihdr));
    file.extend(chunk(b"IDAT", &zlib_store(&raw)));
    file.extend(chunk(b"IEND", &[]));
    file
}

fn round_distance(px: f32, py: f32, span: f32, shape: [f32; 5]) -> f32 {
    let [left, top, width, height, radius] = shape;
    let half_w = width * span * 0.5;
    let half_h = height * span * 0.5;
    let radius = (radius * span).min(half_w).min(half_h);
    let cx = (left + width * 0.5) * span;
    let cy = (top + height * 0.5) * span;
    let dx = (px - cx).abs() - (half_w - radius);
    let dy = (py - cy).abs() - (half_h - radius);
    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    let inside = dx.max(dy).min(0.0);
    outside + inside - radius
}

fn coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

fn over(dst: [f32; 4], rgb: [u8; 3], src_a: f32) -> [f32; 4] {
    let out_a = src_a + dst[3] * (1.0 - src_a);
    if out_a <= 0.0 {
        return [0.0; 4];
    }
    let keep = dst[3] * (1.0 - src_a);
    [
        (f32::from(rgb[0]) / 255.0 * src_a + dst[0] * keep) / out_a,
        (f32::from(rgb[1]) / 255.0 * src_a + dst[1] * keep) / out_a,
        (f32::from(rgb[2]) / 255.0 * src_a + dst[2] * keep) / out_a,
        out_a,
    ]
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut hashed = Vec::with_capacity(4 + data.len());
    hashed.extend_from_slice(kind);
    hashed.extend_from_slice(data);
    out.extend_from_slice(&crc32(&hashed).to_be_bytes());
    out
}

/// Zlib wrapper around stored deflate blocks. The icon is small, so skipping
/// compression keeps the writer free of an image crate.
fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut offset = 0;
    while offset < data.len() {
        let n = (data.len() - offset).min(65535);
        let last = offset + n == data.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&(!(n as u16)).to_le_bytes());
        out.extend_from_slice(&data[offset..offset + n]);
        offset += n;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::{pixels, png};

    fn pixel(buf: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * size + x) * 4) as usize;
        [buf[index], buf[index + 1], buf[index + 2], buf[index + 3]]
    }

    #[test]
    fn the_sheet_sits_to_the_right_of_the_screen() {
        let size = 32;
        let buf = pixels(size);
        assert_eq!(pixel(&buf, size, 0, 0)[3], 0, "the tile corners are clear");

        // At 32px the screen covers x=14. The sheet's visible edge is x=24.
        let screen = pixel(&buf, size, 14, 16);
        let sheet = pixel(&buf, size, 24, 16);
        assert!(screen[3] > 200, "the screen is opaque");
        assert!(sheet[3] > 200, "the sheet is opaque");
        let screen_lum = u32::from(screen[0]) + u32::from(screen[1]) + u32::from(screen[2]);
        let sheet_lum = u32::from(sheet[0]) + u32::from(sheet[1]) + u32::from(sheet[2]);
        assert!(
            sheet_lum > screen_lum + 80,
            "the sheet is lighter than the screen"
        );
        assert!(sheet[0] > 220 && sheet[1] > 220);
    }

    #[test]
    fn the_png_is_that_same_picture() {
        let size = 32u32;
        let file = png(size);
        let raw = decode_png(&file);
        let rgba = pixels(size);
        let mut expected = Vec::new();
        let row = (size * 4) as usize;
        for y in 0..size as usize {
            expected.push(0);
            expected.extend_from_slice(&rgba[y * row..(y + 1) * row]);
        }
        assert_eq!(raw, expected);
    }

    fn decode_png(file: &[u8]) -> Vec<u8> {
        assert_eq!(&file[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        let mut offset = 8;
        let mut inflated = None;
        while offset + 12 <= file.len() {
            let len = u32::from_be_bytes(file[offset..offset + 4].try_into().unwrap()) as usize;
            let kind = &file[offset + 4..offset + 8];
            let data = &file[offset + 8..offset + 8 + len];
            let crc = u32::from_be_bytes(
                file[offset + 8 + len..offset + 12 + len]
                    .try_into()
                    .unwrap(),
            );
            let mut hashed = kind.to_vec();
            hashed.extend_from_slice(data);
            assert_eq!(crc, super::crc32(&hashed));
            if kind == b"IDAT" {
                inflated = Some(inflate_stored(data));
            }
            offset += 12 + len;
            if kind == b"IEND" {
                break;
            }
        }
        inflated.expect("png has an image")
    }

    fn inflate_stored(zlib: &[u8]) -> Vec<u8> {
        assert_eq!(&zlib[..2], &[0x78, 0x01]);
        let mut offset = 2;
        let mut out = Vec::new();
        loop {
            let last = zlib[offset] == 1;
            let n = u16::from_le_bytes(zlib[offset + 1..offset + 3].try_into().unwrap()) as usize;
            let nlen = u16::from_le_bytes(zlib[offset + 3..offset + 5].try_into().unwrap());
            assert_eq!(nlen, !(n as u16));
            offset += 5;
            out.extend_from_slice(&zlib[offset..offset + n]);
            offset += n;
            if last {
                break;
            }
        }
        let sum = u32::from_be_bytes(zlib[offset..offset + 4].try_into().unwrap());
        assert_eq!(sum, super::adler32(&out));
        out
    }
}
