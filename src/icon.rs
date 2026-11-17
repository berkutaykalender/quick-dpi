#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Brand,
    On,
    Off,
    Error,
}

impl Tone {
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Tone::Brand => [0x29, 0x69, 0xe0],
            Tone::On => [0x16, 0xa3, 0x4a],
            Tone::Off => [0x64, 0x74, 0x8b],
            Tone::Error => [0xdc, 0x26, 0x26],
        }
    }

    fn at(self, t: f32) -> [f32; 3] {
        match self {
            Tone::Brand => {
                let (a, b) = ([0x3b as f32, 0x8b as f32, 0xff as f32], [0x17 as f32, 0x47 as f32, 0xc2 as f32]);
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
            }
            _ => self.rgb().map(|c| c as f32),
        }
    }
}

fn cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    let f = |a: f32, b: f32, c: f32, d: f32| u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d;
    (f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1))
}

fn shield() -> &'static [(f32, f32)] {
    static S: std::sync::OnceLock<Vec<(f32, f32)>> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        let mut v = vec![(128.0, 44.0), (186.0, 60.0), (186.0, 126.0)];
        for i in 1..=24 {
            v.push(cubic((186.0, 126.0), (186.0, 170.0), (161.0, 197.0), (128.0, 214.0), i as f32 / 24.0));
        }
        for i in 1..=24 {
            v.push(cubic((128.0, 214.0), (95.0, 197.0), (70.0, 170.0), (70.0, 126.0), i as f32 / 24.0));
        }
        v.push((70.0, 60.0));
        v
    })
}

fn in_poly(x: f32, y: f32, p: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = p.len() - 1;
    for i in 0..p.len() {
        let ((xi, yi), (xj, yj)) = (p[i], p[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

const BARS: [(f32, f32, f32); 4] = [(96.0, 160.0, 98.0), (96.0, 124.0, 128.0), (136.0, 160.0, 128.0), (96.0, 142.0, 158.0)];

fn glyph(x: f32, y: f32) -> bool {
    let (x, y) = (x * 256.0, y * 256.0);
    if !in_poly(x, y, shield()) {
        return false;
    }
    let r = 7.5;
    !BARS.iter().any(|&(x1, x2, by)| {
        let cx = x.clamp(x1 + r, x2 - r);
        (x - cx).powi(2) + (y - by).powi(2) <= r * r
    })
}

fn bg(x: f32, y: f32) -> bool {
    let r = 56.0 / 256.0;
    let qx = (x - 0.5).abs() - (0.5 - r);
    let qy = (y - 0.5).abs() - (0.5 - r);
    let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0);
    out <= r
}

pub fn rgba(size: u32, tone: Tone) -> Vec<u8> {
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    let n = 4;
    for py in 0..size {
        for px in 0..size {
            let (mut cov_bg, mut cov_fg) = (0u32, 0u32);
            for sy in 0..n {
                for sx in 0..n {
                    let x = (px as f32 + (sx as f32 + 0.5) / n as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / n as f32) / size as f32;
                    if bg(x, y) {
                        cov_bg += 1;
                        if glyph(x, y) {
                            cov_fg += 1;
                        }
                    }
                }
            }
            let a = cov_bg as f32 / (n * n) as f32;
            let f = if cov_bg > 0 { cov_fg as f32 / cov_bg as f32 } else { 0.0 };
            let base = tone.at(((px + py) as f32 / (2 * size) as f32).clamp(0.0, 1.0));
            let mix = |c: f32| (c * (1.0 - f) + 255.0 * f).round() as u8;
            out.extend_from_slice(&[mix(base[0]), mix(base[1]), mix(base[2]), (a * 255.0).round() as u8]);
        }
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

pub fn png(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in rgba.chunks_exact((size * 4) as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, c) in chunks.iter().enumerate() {
        z.push((i + 1 == chunks.len()) as u8);
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |typ: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = typ.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

#[allow(dead_code)]
pub fn ico(sizes: &[u32], tone: Tone) -> Vec<u8> {
    let imgs: Vec<Vec<u8>> = sizes.iter().map(|&s| png(s, &rgba(s, tone))).collect();
    let mut out = vec![0, 0, 1, 0];
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut off = 6 + 16 * sizes.len() as u32;
    for (s, img) in sizes.iter().zip(&imgs) {
        let d = if *s >= 256 { 0 } else { *s as u8 };
        out.extend_from_slice(&[d, d, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&off.to_le_bytes());
        off += img.len() as u32;
    }
    for img in imgs {
        out.extend_from_slice(&img);
    }
    out
}
