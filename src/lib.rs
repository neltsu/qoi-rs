use std::num::Wrapping;

#[derive(Debug, Clone, Copy)]
enum QoiOp {
    RGB { r: u8, g: u8, b: u8 },
    RGBA { r: u8, g: u8, b: u8, a: u8 },
    Index { idx: u8 },                     // 6-bit index
    Diff { dr: u8, dg: u8, db: u8 },       // 2-bit differences, bias of 2
    Luma { dg: u8, dr_dg: u8, db_dg: u8 }, // dg - 6-bit (bias of 32), dr_dg and db_dg - 4-bit (bias of 8)
    Run { len: u8 },                       // 6-bit, in [1..62] with bias of -1
}

impl QoiOp {
    fn append_bytes(&self, buf: &mut Vec<u8>) {
        match self {
            &QoiOp::RGB { r, g, b } => buf.extend([0b11111110, r, g, b]),
            &QoiOp::RGBA { r, g, b, a } => buf.extend([0b11111111, r, g, b, a]),
            &QoiOp::Index { idx } => {
                assert!(idx <= 62);
                buf.push((0b00 << 6) | idx)
            }
            &QoiOp::Diff { dr, dg, db } => {
                assert!(dr <= 3 && dg <= 3 && db <= 3);
                buf.push((0b01 << 6) | (dr << 4) | (dg << 2) | (db << 0))
            }
            &QoiOp::Luma { dg, dr_dg, db_dg } => {
                assert!(dg < 64 && dr_dg < 16 && db_dg < 16);
                buf.push((0b10 << 6) | dg);
                buf.push((dr_dg << 4) | db_dg);
            }
            &QoiOp::Run { len } => {
                assert!(len <= 62);
                buf.push((0b11 << 6) | (len - 1))
            }
        }
    }

    fn from_bytes(buf: &[u8]) -> Option<(Self, &[u8])> {
        let (head, rest) = buf.split_first()?;
        match (head >> 6, head & 0b00111111) {
            (0b11, 0b111110) => {
                let (&r, rest) = rest.split_first()?;
                let (&g, rest) = rest.split_first()?;
                let (&b, rest) = rest.split_first()?;
                Some((QoiOp::RGB { r, g, b }, rest))
            }
            (0b11, 0b111111) => {
                let (&r, rest) = rest.split_first()?;
                let (&g, rest) = rest.split_first()?;
                let (&b, rest) = rest.split_first()?;
                let (&a, rest) = rest.split_first()?;
                Some((QoiOp::RGBA { r, g, b, a }, rest))
            }
            (0b00, idx) => {
                Some((QoiOp::Index { idx }, rest))
            }
            (0b01, data) => {
                let dr = (data >> 4) & 0b11;
                let dg = (data >> 2) & 0b11;
                let db = (data >> 0) & 0b11;
                Some((QoiOp::Diff { dr, dg, db }, rest))
            }
            (0b10, dg) => {
                let (next, rest) = rest.split_first()?;
                let dr_dg = (next >> 4) & 0b1111;
                let db_dg = (next >> 0) & 0b1111;
                Some((QoiOp::Luma { dg, dr_dg, db_dg }, rest))
            }
            (0b11, len) => {
                let len = len + 1;
                Some((QoiOp::Run { len }, rest))
            }
            (4..=u8::MAX, _) => unreachable!("(u8) >> 6 cannot be 4 or greater")
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
pub struct Image<T> {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<T>,
}

impl Pixel {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    fn hash(&self) -> u8 {
        let &Pixel { r, g, b, a } = self;
        let hash = (Wrapping(r) * Wrapping(3)
                  + Wrapping(g) * Wrapping(5)
                  + Wrapping(b) * Wrapping(7)
                  + Wrapping(a) * Wrapping(11)) % Wrapping(64);
        hash.0
    }

    pub fn to_bytes(&self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

pub struct Encoder {
    width: u32,
    height: u32,
    channels: u8,
    colorspace: u8,
    cache: [Pixel; 64],
    prev: Pixel,
}

impl Encoder {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            channels: 4,
            colorspace: 0,
            cache: [Pixel::new(0, 0, 0, 255); 64],
            prev: Pixel::new(0, 0, 0, 255),
        }
    }

    fn append_header(&self, buf: &mut Vec<u8>) {
        buf.extend(b"qoif");
        buf.extend(self.width.to_be_bytes());
        buf.extend(self.height.to_be_bytes());
        buf.push(self.channels);
        buf.push(self.colorspace);
    }

    pub fn encode(&mut self, img: &[Pixel]) -> Vec<u8> {
        let mut buf = vec![];

        // header
        self.append_header(&mut buf);

        let mut is_running = false;
        let mut run_length = 0;
        let mut ops = Vec::<QoiOp>::new();

        // body
        for pixel in img {
            let prev = self.prev;
            self.prev = *pixel;
            let &Pixel { r, g, b, a } = pixel;
            let &Pixel { r: pr, g: pg, b: pb, a: pa } = &prev;

            if is_running {
                if prev.eq(pixel) {
                    if run_length >= 62 {
                        ops.push(QoiOp::Run { len: 62 });
                        run_length -= 62;
                    }
                    run_length += 1;
                    continue;
                } else {
                    is_running = false;
                    if run_length > 0 {
                        ops.push(QoiOp::Run { len: run_length });
                    }
                }
            }

            if prev.eq(pixel) {
                assert!(!is_running);
                is_running = true;
                run_length = 1;
                continue;
            }

            let h = pixel.hash();

            if self.cache[h as usize].eq(pixel) {
                ops.push(QoiOp::Index { idx: h });
                continue;
            }

            let Wrapping(dr) = Wrapping(r) - Wrapping(pr) + Wrapping(2);
            let Wrapping(dg) = Wrapping(g) - Wrapping(pg) + Wrapping(2);
            let Wrapping(db) = Wrapping(b) - Wrapping(pb) + Wrapping(2);
            let Wrapping(da) = Wrapping(a) - Wrapping(pa);

            if da == 0 && 0 <= dr && dr <= 3 && 0 <= dg && dg <= 3 && 0 <= db && db <= 3 {
                ops.push(QoiOp::Diff { dr, dg, db });
                continue;
            }

            let Wrapping(dg) = Wrapping(g) - Wrapping(pg);
            let Wrapping(dr) = Wrapping(r) - Wrapping(pr);
            let Wrapping(db) = Wrapping(b) - Wrapping(pb);
            let Wrapping(dr_dg) = Wrapping(8u8) + Wrapping(dr) - Wrapping(dg);
            let Wrapping(db_dg) = Wrapping(8u8) + Wrapping(db) - Wrapping(dg);
            let Wrapping(dg) = Wrapping(32u8) + Wrapping(dg);

            if da == 0 && 0 <= dg && dg < 64 && 0 <= dr_dg && dr_dg < 16 && 0 <= db_dg && db_dg < 16
            {
                ops.push(QoiOp::Luma { dg, dr_dg, db_dg, });
                continue;
            }

            if da == 0 {
                ops.push(QoiOp::RGB { r, g, b });
            } else {
                ops.push(QoiOp::RGBA { r, g, b, a });
            }
        }

        if is_running {
            ops.push(QoiOp::Run { len: run_length });
        }

        for op in ops {
            op.append_bytes(&mut buf);
        }

        // footer
        buf.extend_from_slice(&[0u8, 0, 0, 0, 0, 0, 0, 1]);

        buf
    }
}

pub struct Decoder {
    cache: [Pixel; 64],
    prev: Pixel,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            cache: [Pixel::new(0, 0, 0, 255); 64],
            prev: Pixel::new(0, 0, 0, 255),
        }
    }

    pub fn decode(&mut self, data: &[u8]) -> Option<Image<Pixel>> {
        // header
        let (magic, data) = data.split_at_checked(4)?;
        if !magic.eq(b"qoif") {
            return None;
        }

        let (width_bytes, data) = data.split_first_chunk::<4>()?;
        let width = u32::from_be_bytes(*width_bytes);
        let (height_bytes, data) = data.split_first_chunk::<4>()?;
        let height = u32::from_be_bytes(*height_bytes);

        let (_channels, data) = data.split_first()?;
        let (_colorspace, data) = data.split_first()?;

        // body
        let mut data = data;
        let mut pixels = Vec::<Pixel>::with_capacity((width * height) as usize);
        while pixels.len() < (width * height) as usize {
            let (op, rest) = QoiOp::from_bytes(data)?;
            let mut count: u8 = 1;
            let pixel = match op {
                QoiOp::RGB { r, g, b } => {
                    let a = self.prev.a;
                    Pixel::new(r, g, b, a)
                }
                QoiOp::RGBA { r, g, b, a } => {
                    Pixel::new(r, g, b, a)
                }
                QoiOp::Index { idx } => {
                    *self.cache.get(idx as usize)?
                }
                QoiOp::Diff { dr, dg, db } => {
                    let Pixel { r: pr, g: pg, b: pb, a } = self.prev;
                    let Wrapping(r) = Wrapping(pr) + Wrapping(dr) - Wrapping(2);
                    let Wrapping(g) = Wrapping(pg) + Wrapping(dg) - Wrapping(2);
                    let Wrapping(b) = Wrapping(pb) + Wrapping(db) - Wrapping(2);
                    Pixel::new(r, g, b, a)
                }
                QoiOp::Luma { dg, dr_dg, db_dg } => {
                    let Wrapping(dg) = Wrapping(dg) - Wrapping(32);
                    let Wrapping(dr) = Wrapping(dr_dg) + Wrapping(dg) - Wrapping(8);
                    let Wrapping(db) = Wrapping(db_dg) + Wrapping(dg) - Wrapping(8);
                    let Pixel { r: pr, g: pg, b: pb, a } = self.prev;
                    let Wrapping(r) = Wrapping(pr) + Wrapping(dr);
                    let Wrapping(g) = Wrapping(pg) + Wrapping(dg);
                    let Wrapping(b) = Wrapping(pb) + Wrapping(db);
                    Pixel::new(r, g, b, a)
                }
                QoiOp::Run { len } => {
                    count = len;
                    self.prev
                }
            };
            self.prev = pixel;
            let h = pixel.hash();
            self.cache[h as usize] = pixel;
            data = rest;

            for _ in 0..count {
                pixels.push(pixel);
            }
        }

        if pixels.len() > (width * height) as usize {
            return None;
        }

        // footer
        if [0u8, 0, 0, 0, 0, 0, 0, 1].ne(data) {
            return None;
        }

        Some(Image {
            width: width as usize,
            height: height as usize,
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};
    use std::time::Instant;

    #[test]
    fn test() {
        use super::*;

        let now = Instant::now();
        let img = image::ImageReader::open("assets/suz.png").unwrap().decode().unwrap();
        println!("PNG decoder took {} us", now.elapsed().as_micros());

        let mut encoder = Encoder::new(img.width(), img.height());

        let img_buf = img.as_rgba8().unwrap()
            .pixels()
            .map(|&Rgba::<u8>([r, g, b, a])| Pixel::new(r, g, b, a))
            .collect::<Vec<_>>();

        let now = Instant::now();
        let data = encoder.encode(&img_buf);
        std::fs::write("encoded.qoi", &data).unwrap();
        println!("QOI encoder took {} us", now.elapsed().as_micros());

        let now = Instant::now();
        img.save("encoded.png").unwrap();
        println!("PNG encoder took {} us", now.elapsed().as_micros());

        let now = Instant::now();
        let mut decoder = Decoder::new();
        let data = std::fs::read("encoded.qoi").unwrap();
        let decoded = decoder.decode(&data).unwrap();
        println!("QOI decoder took {} us", now.elapsed().as_micros());

        assert!(decoded.pixels.eq(&img_buf));

        let buf = decoded.pixels.iter().flat_map(Pixel::to_bytes).collect::<Vec<_>>();
        RgbaImage::from_vec(img.width(), img.height(), buf)
            .unwrap()
            .save("decoded.png")
            .unwrap();
    }
}
