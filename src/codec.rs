//! Text codecs for `--encode` / `--decode`: base64, hex and URL percent
//! encoding of a piece of text, taken as its UTF-8 bytes. Pure functions
//! over bytes, written out by hand — each is a few lines, not worth a
//! dependency.

use std::fmt;

/// A way of writing bytes as plain ASCII text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// RFC 4648 base64, standard alphabet, padded with `=`.
    Base64,
    /// Two lowercase hex digits per byte.
    Hex,
    /// RFC 3986 percent encoding: every byte but the unreserved
    /// characters (`A-Z a-z 0-9 - . _ ~`) as `%XX`.
    Url,
}

impl Codec {
    /// The name it is given on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Codec::Base64 => "base64",
            Codec::Hex => "hex",
            Codec::Url => "url",
        }
    }

    /// Write `bytes` in this codec.
    pub fn encode(self, bytes: &[u8]) -> String {
        match self {
            Codec::Base64 => encode_base64(bytes),
            Codec::Hex => bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            Codec::Url => encode_url(bytes),
        }
    }

    /// Read `text` back into the bytes it encodes.
    pub fn decode(self, text: &str) -> Result<Vec<u8>, DecodeError> {
        let decoded = match self {
            Codec::Base64 => decode_base64(text),
            Codec::Hex => decode_hex(text),
            Codec::Url => decode_url(text),
        };
        decoded.ok_or_else(|| DecodeError {
            codec: self,
            text: text.to_owned(),
        })
    }
}

/// Text that is not valid in the codec it was to be decoded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    codec: Codec,
    text: String,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not valid {}", self.text, self.codec.name())
    }
}

impl std::error::Error for DecodeError {}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_base64(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        //three bytes are four six-bit digits; a short chunk fills the
        //digits it has and pads the rest
        let group = chunk
            .iter()
            .enumerate()
            .fold(0u32, |group, (index, &byte)| {
                group | u32::from(byte) << (16 - 8 * index)
            });
        for digit in 0..4 {
            match digit <= chunk.len() {
                true => result.push(char::from(
                    BASE64[(group >> (18 - 6 * digit) & 0x3F) as usize],
                )),
                false => result.push('='),
            }
        }
    }
    result
}

/// Padding is optional; anything outside the alphabet, or a length no
/// encoding produces, is not base64.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let digits = text.trim_end_matches('=').as_bytes();
    if text.len() - digits.len() > 2 || digits.len() % 4 == 1 {
        return None;
    }
    let mut bytes = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let mut group = 0u32;
        for (index, &digit) in chunk.iter().enumerate() {
            let value = BASE64
                .iter()
                .position(|&known| known == digit)?;
            group |= (value as u32) << (18 - 6 * index);
        }
        //n digits carry n - 1 whole bytes
        bytes.extend((0..chunk.len() - 1).map(|index| (group >> (16 - 8 * index)) as u8));
    }
    Some(bytes)
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    let digits = text.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks(2)
        .map(|pair| Some(hex_value(pair[0])? << 4 | hex_value(pair[1])?))
        .collect()
}

fn hex_value(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .map(|value| value as u8)
}

fn encode_url(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len());
    for &byte in bytes {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                result.push(char::from(byte))
            }
            _ => result.push_str(&format!("%{byte:02X}")),
        }
    }
    result
}

/// `%XX` becomes its byte, anything else stands for itself — `+` too,
/// which only form encoding reads as a space.
fn decode_url(text: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        match byte {
            b'%' => {
                let [high, low, ..] = *after else {
                    return None;
                };
                bytes.push(hex_value(high)? << 4 | hex_value(low)?);
                rest = &after[2..];
            }
            _ => {
                bytes.push(byte);
                rest = after;
            }
        }
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_vectors() {
        // Given the RFC 4648 test vectors
        let vectors = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];

        // When / Then both ways
        for (plain, encoded) in vectors {
            assert_eq!(Codec::Base64.encode(plain.as_bytes()), encoded);
            assert_eq!(Codec::Base64.decode(encoded).unwrap(), plain.as_bytes());
        }
    }

    #[test]
    fn base64_padding_is_optional_but_the_alphabet_is_not() {
        // Given / When / Then
        assert_eq!(Codec::Base64.decode("Zg").unwrap(), b"f");
        assert!(Codec::Base64.decode("Z").is_err());
        assert!(Codec::Base64.decode("Zm9v!").is_err());
        assert!(Codec::Base64.decode("Zg===").is_err());
    }

    #[test]
    fn hex_reads_either_case_and_writes_lowercase() {
        // Given / When / Then
        assert_eq!(Codec::Hex.encode("ż".as_bytes()), "c5bc");
        assert_eq!(Codec::Hex.decode("C5bC").unwrap(), "ż".as_bytes());
        assert!(Codec::Hex.decode("abc").is_err());
        assert!(Codec::Hex.decode("zz").is_err());
    }

    #[test]
    fn url_encoding_keeps_only_the_unreserved_characters() {
        // Given / When / Then
        assert_eq!(Codec::Url.encode("a b/ż~".as_bytes()), "a%20b%2F%C5%BC~");
        assert_eq!(
            Codec::Url
                .decode("a%20b+%c5%BC")
                .unwrap(),
            "a b+ż".as_bytes()
        );
        assert!(Codec::Url.decode("100%").is_err());
        assert!(Codec::Url.decode("%2").is_err());
        assert!(Codec::Url.decode("%zz").is_err());
    }

    #[test]
    fn a_decode_error_names_the_text_and_the_codec() {
        // Given / When
        let error = Codec::Hex.decode("xyz").unwrap_err();

        // Then
        assert_eq!(error.to_string(), "`xyz` is not valid hex");
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn every_codec_decodes_what_it_encodes(bytes in proptest::collection::vec(any::<u8>(), 0..40)) {
            for codec in [Codec::Base64, Codec::Hex, Codec::Url] {
                let encoded = codec.encode(&bytes);
                prop_assert!(encoded.is_ascii());
                prop_assert_eq!(codec.decode(&encoded).unwrap(), bytes.clone());
            }
        }
    }
}
