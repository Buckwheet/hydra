//! Entity decoding for the XML-ish GemStone wire format.
//!
//! Recovered behaviour from the binary:
//!   * named entities: `amp`, `lt`, `gt`, `quot`, `apos`, `nbsp`
//!   * numeric references: `&#123;`, `&#x7b;` / `&#X7B;`
//!   * unknown or malformed references are copied through verbatim
//!   * numeric values above `U+10FFFF` and surrogates are rejected (copied
//!     through verbatim)
//!   * `strip_control_chars` keeps tab/newline and drops other C0 controls
//!     (the exact set of dropped characters is partially inferred; see the
//!     `UNKNOWN` notes below)

/// Decode the five XML entities plus `&nbsp;` and numeric character
/// references.
///
/// Malformed references are left untouched, so the function is safe to apply
/// to arbitrary wire text.
pub fn decode_entities(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'&' {
            // Copy the next UTF-8 code point as-is.
            let ch = input[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }

        // Find the terminating ';'.  The original only looks at the next 12
        // bytes for the ';', so absurdly long runs are copied literally.
        let rest = &input[i..];
        let semi = rest
            .char_indices()
            .take(13)
            .find_map(|(idx, c)| (c == ';').then_some(idx));

        let Some(semi) = semi else {
            out.push('&');
            i += 1;
            continue;
        };

        let body = &rest[1..semi]; // between '&' and ';'
        if body.is_empty() {
            out.push('&');
            i += 1;
            continue;
        }

        let decoded = if let Some(num) = body.strip_prefix('#') {
            decode_numeric(num)
        } else {
            decode_named(body)
        };

        if let Some(ch) = decoded {
            out.push(ch);
            i += semi + 1;
        } else {
            // Unknown reference: preserve it verbatim.
            out.push('&');
            i += 1;
        }
    }
    out
}

/// Decode a numeric character reference body (the part after `#`).
fn decode_numeric(body: &str) -> Option<char> {
    // `&#x...;` / `&#X...;` is hexadecimal, everything else decimal.
    let (digits, radix) = match body.as_bytes().first() {
        Some(b'x') | Some(b'X') => (&body[1..], 16),
        _ => (body, 10),
    };
    if digits.is_empty() {
        return None;
    }
    // The binary trims nothing but rejects an explicit sign.
    if digits.starts_with('+') || digits.starts_with('-') {
        return None;
    }
    let mut value: u32 = 0;
    for c in digits.chars() {
        let digit = c.to_digit(radix)?;
        value = value.checked_mul(radix)?.checked_add(digit)?;
        if value > 0x0011_0000 {
            return None;
        }
    }
    // Reject surrogate halves and out-of-range values, matching the binary.
    if (0xD800..=0xDFFF).contains(&value) || value == 0x0011_0000 {
        return None;
    }
    char::from_u32(value)
}

fn decode_named(body: &str) -> Option<char> {
    match body {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        // GemStone emits `&nbsp;` for a non-breaking space.
        "nbsp" => Some('\u{00a0}'),
        _ => None,
    }
}

/// Decode an attribute value.
///
/// The binary first runs [`decode_entities`] and then normalises control
/// characters.  Attribute values in the wire format are always quoted, so
/// the decoded result is returned without the surrounding quotes.
pub fn decode_attribute_value(input: &str) -> String {
    let decoded = decode_entities(input);
    strip_control_chars(&decoded)
}

/// Remove C0 control characters from `input`.
///
/// The binary's loop explicitly preserves `\t` and keeps ordinary text; the
/// remaining control characters are dropped.  `\n`/`\r` do not normally occur
/// inside an attribute value.  `// UNKNOWN:` the exact handling of `0x7f` and
/// the `0x80..=0x9f` range was not fully resolved from the disassembly; this
/// implementation keeps `0x7f..=0x9f` and drops the rest of C0.
pub fn strip_control_chars(input: &str) -> String {
    input
        .chars()
        .filter(|&c| {
            if c == '\t' || c == '\n' || c == '\r' {
                return true;
            }
            !((c as u32) < 0x20)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_basic_named_entities() {
        assert_eq!(decode_entities("a &lt; b &gt; c"), "a < b > c");
        assert_eq!(decode_entities("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(decode_entities("&quot;hi&quot;"), "\"hi\"");
        assert_eq!(decode_entities("it&apos;s"), "it's");
        assert_eq!(decode_entities("a&nbsp;b"), "a\u{00a0}b");
    }

    #[test]
    fn decodes_numeric_and_hex_refs() {
        assert_eq!(decode_entities("&#65;&#66;&#67;"), "ABC");
        assert_eq!(decode_entities("&#x41;&#X42;&#x43;"), "ABC");
        assert_eq!(decode_entities("&#9731;"), "\u{2603}");
    }

    #[test]
    fn leaves_bad_refs_untouched() {
        assert_eq!(decode_entities("&bogus;"), "&bogus;");
        assert_eq!(decode_entities("&#xZZ;"), "&#xZZ;");
        assert_eq!(decode_entities("&amp"), "&amp");
        assert_eq!(decode_entities("100% & more"), "100% & more");
    }

    #[test]
    fn rejects_surrogates_and_out_of_range() {
        assert_eq!(decode_entities("&#xD800;"), "&#xD800;");
        assert_eq!(decode_entities("&#1114112;"), "&#1114112;");
        assert_eq!(decode_entities("&#x110000;"), "&#x110000;");
    }

    #[test]
    fn decodes_attribute_values() {
        assert_eq!(
            decode_attribute_value("a &quot;blue&quot; gem"),
            "a \"blue\" gem"
        );
        assert_eq!(decode_attribute_value("line&#10;two"), "line\ntwo");
    }

    #[test]
    fn strips_control_chars_but_keeps_tabs() {
        assert_eq!(strip_control_chars("a\u{0}b\u{1}c"), "abc");
        assert_eq!(strip_control_chars("a\tb"), "a\tb");
        assert_eq!(strip_control_chars("a\u{7f}b"), "a\u{7f}b");
    }
}
