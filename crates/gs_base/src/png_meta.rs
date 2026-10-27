//! Minimal PNG ancillary-chunk reader.
//!
//! Recovered from `gs_base::png_meta::read_text_bytes` @0x100a8156c and
//! `gs_base::png_meta::chunks` @0x100a817a4 (local symbol, inlined in the
//! workspace crate `png`).
//!
//! ### `chunks`
//!
//! ```text
//! if data.len() < 8 || data[..8] != PNG_SIG { return None }
//! pos = 8
//! loop {
//!     if pos + 12 > data.len()      { break }
//!     len   = u32::from_be_bytes(data[pos..pos+4]) as usize   // `rev w25, w9`
//!     kind  = data[pos+4..pos+8]                              // compared to "IEND"
//!     body  = data[pos+8..pos+8+len]                          // bounds-checked
//!     out.push(Chunk { kind, body })
//!     pos  += 12 + len                                        // 4 len + 4 kind + 4 CRC
//! }
//! ```
//!
//! The eight-byte signature constant in the binary is `0x0A1A0A0D474E5089`,
//! i.e. `89 50 4E 47 0D 0A 1A 0A`.  A chunk whose declared length runs past the
//! end of the buffer yields `None` (the binary's `bail` path); running out of
//! bytes before a 12-byte header simply stops the scan.
//!
//! ### `read_text_bytes`
//!
//! Walks the chunks, keeps only `tEXt`, and for each one requires
//!
//! * a NUL terminator inside the payload (`cbz x26` — an empty payload is a
//!   hard `None`, not a skip);
//! * `payload[..nul].len() == keyword.len()`;
//! * `memcmp(payload, keyword, keyword.len()) == 0`.
//!
//! The text is `payload[nul+1..]`; the binary copies it into a fresh `String`
//! and runs `str::from_utf8`, returning `None` when the text is not UTF-8.
//!
//! Only `tEXt` is handled — `iTXt`/`zTXt` are *not* in the disassembly, so no
//! inflate or UTF-8-with-language-tag path is implemented here.

/// The 8-byte PNG signature.
pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// A single PNG chunk, borrowed from the input buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk<'a> {
    /// The four chunk-type bytes, e.g. `b"tEXt"`.
    pub kind: [u8; 4],
    /// Chunk payload (length taken from the big-endian length field).
    pub body: &'a [u8],
}

/// Split `data` into PNG chunks, stopping after `IEND`.
///
/// Returns `None` when `data` is not PNG, or when a chunk's declared length
/// runs past the end of the buffer.
pub fn chunks(data: &[u8]) -> Option<Vec<Chunk<'_>>> {
    if data.len() < 8 || data[..8] != PNG_SIGNATURE {
        return None;
    }
    let mut out = Vec::new();
    let mut pos = 8usize;
    loop {
        // `add x8, x21, #0xc; cmp x8, x19; b.hi break`
        if pos + 12 > data.len() {
            break;
        }
        let len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
            as usize;
        let mut kind = [0u8; 4];
        kind.copy_from_slice(&data[pos + 4..pos + 8]);
        let body_start = pos + 8;
        let body_end = body_start.checked_add(len)?;
        if body_end > data.len() {
            return None;
        }
        out.push(Chunk {
            kind,
            body: &data[body_start..body_end],
        });
        pos = pos + 12 + len;
        if &kind == b"IEND" {
            break;
        }
    }
    Some(out)
}

/// The UTF-8 text of the first `tEXt` chunk whose keyword matches.
///
/// `@0x100a8156c` — `read_text_bytes(data: &[u8], keyword: &str) -> Option<String>`.
pub fn read_text_bytes(data: &[u8], keyword: &str) -> Option<String> {
    for chunk in chunks(data)? {
        if &chunk.kind != b"tEXt" {
            continue;
        }
        // An empty payload or a missing NUL is a hard failure, not a skip.
        let nul = chunk.body.iter().position(|&b| b == 0)?;
        if nul != keyword.len() {
            continue;
        }
        // The binary uses a plain `memcmp`, i.e. an exact byte comparison.
        if chunk.body[..nul] != *keyword.as_bytes() {
            continue;
        }
        let text = &chunk.body[nul + 1..];
        return std::str::from_utf8(text).ok().map(|s| s.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a PNG-ish byte stream from `(kind, body)` pairs.
    fn png(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut v = PNG_SIGNATURE.to_vec();
        for (kind, body) in chunks {
            v.extend_from_slice(&(body.len() as u32).to_be_bytes());
            v.extend_from_slice(*kind);
            v.extend_from_slice(body);
            v.extend_from_slice(&[0, 0, 0, 0]); // CRC (not verified)
        }
        v
    }

    fn text(keyword: &str, text: &str) -> Vec<u8> {
        let mut b = keyword.as_bytes().to_vec();
        b.push(0);
        b.extend_from_slice(text.as_bytes());
        b
    }

    #[test]
    fn reads_text_chunk() {
        let body = text("Title", "Hydra map");
        let data = png(&[(b"IHDR", &[0; 13]), (b"tEXt", &body), (b"IEND", &[])]);
        assert_eq!(
            read_text_bytes(&data, "Title").as_deref(),
            Some("Hydra map")
        );
        assert_eq!(read_text_bytes(&data, "title"), None);
        assert_eq!(read_text_bytes(&data, "Author"), None);
    }

    #[test]
    fn skips_other_chunk_types_and_picks_the_right_one() {
        let data = png(&[
            (b"IHDR", &[1, 2, 3]),
            (b"tEXt", &text("Author", "nisugi")),
            (b"zTXt", &text("Title", "compressed")),
            (b"tEXt", &text("Title", "Hydra")),
            (b"IEND", &[]),
        ]);
        assert_eq!(read_text_bytes(&data, "Title").as_deref(), Some("Hydra"));
        assert_eq!(read_text_bytes(&data, "Author").as_deref(), Some("nisugi"));
    }

    #[test]
    fn keyword_must_match_exactly() {
        let data = png(&[(b"tEXt", &text("Title", "x")), (b"IEND", &[])]);
        // A shorter prefix is not a match (length is compared first).
        assert_eq!(read_text_bytes(&data, "Tit"), None);
        assert_eq!(read_text_bytes(&data, "TitleLong"), None);
    }

    #[test]
    fn truncated_and_malformed_inputs_return_none() {
        assert_eq!(read_text_bytes(&[], "Title"), None);
        assert_eq!(read_text_bytes(b"not a png at all", "Title"), None);
        // Signature only: no chunks -> None (not a panic).
        assert_eq!(read_text_bytes(&PNG_SIGNATURE, "Title"), None);
        // Declared length past the end.
        let mut bad = PNG_SIGNATURE.to_vec();
        bad.extend_from_slice(&100u32.to_be_bytes());
        bad.extend_from_slice(b"tEXt");
        assert_eq!(read_text_bytes(&bad, "Title"), None);
        // Payload with no NUL.
        let nonul = png(&[(b"tEXt", b"TitleHYDRA"), (b"IEND", &[])]);
        assert_eq!(read_text_bytes(&nonul, "Title"), None);
        // Payload whose NUL is at index 0 but keyword is non-empty.
        let leading = png(&[(b"tEXt", b"\0Hydra"), (b"IEND", &[])]);
        assert_eq!(read_text_bytes(&leading, "Title"), None);
    }

    #[test]
    fn non_utf8_text_returns_none() {
        let mut body = text("Title", "");
        body.push(0xFF);
        let data = png(&[(b"tEXt", &body), (b"IEND", &[])]);
        assert_eq!(read_text_bytes(&data, "Title"), None);
    }

    #[test]
    fn chunk_walk() {
        let data = png(&[
            (b"IHDR", &[0; 13]),
            (b"tEXt", b"a\0b"),
            (b"IEND", &[]),
        ]);
        let cs = chunks(&data).unwrap();
        assert_eq!(cs.len(), 3);
        assert_eq!(&cs[0].kind, b"IHDR");
        assert_eq!(cs[0].body.len(), 13);
        assert_eq!(&cs[1].kind, b"tEXt");
        assert_eq!(cs[1].body, b"a\0b");
        assert_eq!(&cs[2].kind, b"IEND");
        assert_eq!(cs[2].body.len(), 0);
    }
}
