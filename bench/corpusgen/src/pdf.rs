// SPDX-License-Identifier: AGPL-3.0-or-later

//! Minimal PDF writer: just enough to emit large synthetic documents fast
//! and deterministically, and to produce deliberately broken files.

use std::fmt::Write as _;
use std::io::Write as _;

use flate2::Compression;
use flate2::write::ZlibEncoder;

/// Object number. Generation is always 0.
pub type ObjId = u32;

pub struct PdfWriter {
    buf: Vec<u8>,
    /// Byte offset of each object, indexed by `ObjId - 1`.
    offsets: Vec<Option<usize>>,
}

impl PdfWriter {
    pub fn new() -> Self {
        let mut buf = Vec::with_capacity(1 << 20);
        // Binary comment marks the file as binary for transfer tools.
        buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
        Self {
            buf,
            offsets: Vec::new(),
        }
    }

    /// Reserves an object number to be written later.
    pub fn alloc(&mut self) -> ObjId {
        self.offsets.push(None);
        self.offsets.len() as ObjId
    }

    pub fn obj(&mut self, id: ObjId, body: &str) {
        self.begin(id);
        self.buf.extend_from_slice(body.as_bytes());
        self.buf.extend_from_slice(b"\nendobj\n");
    }

    /// Writes a stream object; `dict` holds extra entries (without `<< >>`).
    pub fn stream(&mut self, id: ObjId, dict: &str, data: &[u8], compress: bool) {
        let compressed;
        let (data, filter) = if compress {
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::fast());
            enc.write_all(data).unwrap_or_default();
            compressed = enc.finish().unwrap_or_default();
            (compressed.as_slice(), " /Filter /FlateDecode")
        } else {
            (data, "")
        };
        self.begin(id);
        let head = format!("<< {dict}{filter} /Length {} >>\nstream\n", data.len());
        self.buf.extend_from_slice(head.as_bytes());
        self.buf.extend_from_slice(data);
        self.buf.extend_from_slice(b"\nendstream\nendobj\n");
    }

    fn begin(&mut self, id: ObjId) {
        self.offsets[id as usize - 1] = Some(self.buf.len());
        let _ = writeln!(ByteSink(&mut self.buf), "{id} 0 obj");
    }

    /// Writes the xref table and trailer and returns the file bytes.
    pub fn finish(mut self, root: ObjId, info: Option<ObjId>) -> Vec<u8> {
        let xref_at = self.buf.len();
        let mut xref = format!("xref\n0 {}\n0000000000 65535 f\r\n", self.offsets.len() + 1);
        for off in &self.offsets {
            match off {
                Some(o) => {
                    let _ = write!(xref, "{o:010} 00000 n\r\n");
                }
                None => xref.push_str("0000000000 65535 f\r\n"),
            }
        }
        let info = info.map(|i| format!(" /Info {i} 0 R")).unwrap_or_default();
        let _ = write!(
            xref,
            "trailer\n<< /Size {} /Root {root} 0 R{info} >>\nstartxref\n{xref_at}\n%%EOF\n",
            self.offsets.len() + 1
        );
        self.buf.extend_from_slice(xref.as_bytes());
        self.buf
    }
}

struct ByteSink<'a>(&'a mut Vec<u8>);

impl std::fmt::Write for ByteSink<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

/// Deterministic xorshift64* generator: the corpus must be byte-identical everywhere.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        let unit = (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32;
        lo + unit * (hi - lo)
    }

    pub fn below(&mut self, n: u32) -> u32 {
        (self.next_u64() % u64::from(n.max(1))) as u32
    }
}

/// Escapes a string for a PDF literal `( )`.
pub fn pdf_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('(');
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_ascii() => out.push(c),
            _ => out.push('?'),
        }
    }
    out.push(')');
    out
}
