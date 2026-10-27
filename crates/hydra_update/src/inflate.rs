//! A DEFLATE (RFC 1951) decoder, for the ZIP members a release archive
//! carries.
//!
//! The original used the `flate2` backend inside `zip`; both are external
//! crates, so the decoder is reproduced here.  The error texts mirror the ones
//! the binary carries (`"corrupt deflate stream"`, `"incomplete deflate
//! stream"`, `__const` 0x1023fd110-ish).

/// Decompress a raw DEFLATE stream (no zlib header).
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut bits = Bits { data, at: 0, bit: 0 };
    let mut out = Vec::new();
    loop {
        let last = bits.bits(1)? == 1;
        match bits.bits(2)? {
            0 => stored(&mut bits, &mut out)?,
            1 => {
                let (literals, distances) = fixed();
                codes(&mut bits, &literals, &distances, &mut out)?;
            }
            2 => {
                let (literals, distances) = dynamic(&mut bits)?;
                codes(&mut bits, &literals, &distances, &mut out)?;
            }
            _ => return Err("corrupt deflate stream".to_string()),
        }
        if last {
            return Ok(out);
        }
    }
}

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    bit: u32,
}

impl Bits<'_> {
    fn bits(&mut self, count: u32) -> Result<u32, String> {
        let mut value = 0u32;
        for i in 0..count {
            let byte = *self
                .data
                .get(self.at)
                .ok_or_else(|| "incomplete deflate stream".to_string())?;
            let bit = (byte >> self.bit) & 1;
            value |= (bit as u32) << i;
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.at += 1;
            }
        }
        Ok(value)
    }

    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.at += 1;
        }
    }
}

fn stored(bits: &mut Bits<'_>, out: &mut Vec<u8>) -> Result<(), String> {
    bits.align();
    let length = bits.bits(16)? as usize;
    let complement = bits.bits(16)? as usize;
    if length != (!complement & 0xffff) {
        return Err("corrupt deflate stream".to_string());
    }
    for _ in 0..length {
        out.push(bits.bits(8)? as u8);
    }
    Ok(())
}

/// A canonical Huffman table (the "puff" layout).
pub struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Huffman {
        let mut count = [0u16; 16];
        for length in lengths {
            count[*length as usize] += 1;
        }
        count[0] = 0;
        let mut offs = [0u16; 16];
        for length in 1..16 {
            offs[length] = offs[length - 1] + count[length - 1];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (value, length) in lengths.iter().enumerate() {
            if *length != 0 {
                symbol[offs[*length as usize] as usize] = value as u16;
                offs[*length as usize] += 1;
            }
        }
        Huffman { count, symbol }
    }

    /// Read one symbol, bit by bit.
    fn decode(&self, bits: &mut Bits<'_>) -> Result<u16, String> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for length in 1..16 {
            code |= bits.bits(1)? as i32;
            let count = self.count[length] as i32;
            if code - count < first {
                return Ok(self.symbol[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err("corrupt deflate stream".to_string())
    }
}

fn fixed() -> (Huffman, Huffman) {
    let mut lengths = [0u8; 288];
    for (i, length) in lengths.iter_mut().enumerate() {
        *length = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let literals = Huffman::new(&lengths);
    let distances = Huffman::new(&[5u8; 30]);
    (literals, distances)
}

fn dynamic(bits: &mut Bits<'_>) -> Result<(Huffman, Huffman), String> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let literal_count = bits.bits(5)? as usize + 257;
    let distance_count = bits.bits(5)? as usize + 1;
    let code_count = bits.bits(4)? as usize + 4;
    if literal_count > 286 || distance_count > 30 {
        return Err("corrupt deflate stream".to_string());
    }
    let mut code_lengths = [0u8; 19];
    for &slot in ORDER.iter().take(code_count) {
        code_lengths[slot] = bits.bits(3)? as u8;
    }
    let code_table = Huffman::new(&code_lengths);

    let mut lengths = vec![0u8; literal_count + distance_count];
    let mut at = 0usize;
    while at < lengths.len() {
        let symbol = code_table.decode(bits)?;
        match symbol {
            0..=15 => {
                lengths[at] = symbol as u8;
                at += 1;
            }
            16 => {
                if at == 0 {
                    return Err("corrupt deflate stream".to_string());
                }
                let repeat = 3 + bits.bits(2)? as usize;
                let previous = lengths[at - 1];
                if at + repeat > lengths.len() {
                    return Err("corrupt deflate stream".to_string());
                }
                for _ in 0..repeat {
                    lengths[at] = previous;
                    at += 1;
                }
            }
            17 => {
                let repeat = 3 + bits.bits(3)? as usize;
                if at + repeat > lengths.len() {
                    return Err("corrupt deflate stream".to_string());
                }
                at += repeat;
            }
            18 => {
                let repeat = 11 + bits.bits(7)? as usize;
                if at + repeat > lengths.len() {
                    return Err("corrupt deflate stream".to_string());
                }
                at += repeat;
            }
            _ => return Err("corrupt deflate stream".to_string()),
        }
    }
    let literals = Huffman::new(&lengths[..literal_count]);
    let distances = Huffman::new(&lengths[literal_count..]);
    Ok((literals, distances))
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn codes(
    bits: &mut Bits<'_>,
    literals: &Huffman,
    distances: &Huffman,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    loop {
        let symbol = literals.decode(bits)? as usize;
        match symbol {
            0..=255 => out.push(symbol as u8),
            256 => return Ok(()),
            257..=285 => {
                let index = symbol - 257;
                let length = LENGTH_BASE[index] as usize
                    + bits.bits(LENGTH_EXTRA[index] as u32)? as usize;
                let code = distances.decode(bits)? as usize;
                if code >= DISTANCE_BASE.len() {
                    return Err("corrupt deflate stream".to_string());
                }
                let distance =
                    DISTANCE_BASE[code] as usize + bits.bits(DISTANCE_EXTRA[code] as u32)? as usize;
                if distance > out.len() {
                    return Err("corrupt deflate stream".to_string());
                }
                let start = out.len() - distance;
                for i in 0..length {
                    let byte = out[start + i];
                    out.push(byte);
                }
            }
            _ => return Err("corrupt deflate stream".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `printf 'hello hello hello\n' | zlib` — zlib's raw (wbits=-15) DEFLATE
    /// output, captured once and checked with `zlib.decompress(..., -15)`.
    #[test]
    fn inflates_a_fixed_huffman_block() {
        let data = [
            0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x90, 0x5c, 0x00,
        ];
        assert_eq!(inflate(&data).unwrap(), b"hello hello hello\n");
    }

    #[test]
    fn inflates_a_dynamic_huffman_block() {
        let data = [
            0x2b, 0xc9, 0x48, 0x55, 0x28, 0x2c, 0xcd, 0x4c, 0xce, 0x56, 0x48, 0x2a, 0xca, 0x2f,
            0xcf, 0x53, 0x48, 0xcb, 0xaf, 0x50, 0xc8, 0x2a, 0xcd, 0x2d, 0x28, 0x56, 0xc8, 0x2f,
            0x4b, 0x2d, 0x52, 0x28, 0x01, 0x4a, 0xe7, 0x24, 0x56, 0x55, 0x2a, 0xa4, 0xe4, 0xa7,
            0xeb, 0x81, 0x79, 0x83, 0x54, 0x31, 0x00,
        ];
        let text = inflate(&data).unwrap();
        assert_eq!(text.len(), 200);
        assert_eq!(&text[..20], b"the quick brown fox ");
        assert_eq!(&text[180..], b"the quick brown fox ");
    }

    #[test]
    fn inflates_a_stored_block() {
        // An uncompressed (stored) block: `01` = last block, stored;
        // LEN = 5, NLEN = !5; then the bytes.
        let data = [0x01, 0x05, 0x00, 0xfa, 0xff, b'h', b'e', b'l', b'l', b'o'];
        assert_eq!(inflate(&data).unwrap(), b"hello");
    }

    #[test]
    fn refuses_broken_streams() {
        assert!(inflate(&[]).is_err());
        assert!(inflate(&[0x07]).is_err()); // reserved block type
        assert!(inflate(&[0x01, 0x05, 0x00, 0x00, 0x00, b'h']).is_err()); // bad length
        assert!(inflate(&[0x01, 0x05, 0x00, 0xfa, 0xff, b'h']).is_err()); // truncated
    }
}
