//! Text encodings at the edges of a run. The engine works on UTF-8 only;
//! an input in another encoding is decoded into UTF-8 on the way in
//! ([`open`]) and the output encoded on the way out ([`EncodeWriter`]),
//! so nothing between them needs to know.
//!
//! A byte order mark says how a file is encoded, not what it holds. Read
//! as text, a UTF-8 BOM is an invisible U+FEFF glued to the first line,
//! which then counts as an extra column, makes the first field fail to
//! match `^…$` and the first number fail to parse. So it is taken off
//! every input — per input file, since every file of a concatenation may
//! carry one — and an in-place edit puts it back. A UTF-16 BOM also
//! settles the encoding, whatever was declared, as it does in a browser.
//!
//! Neither direction replaces what it cannot convert: an input that is
//! not valid in its encoding, or a character the output encoding has no
//! byte for, is an error — a silently substituted `?` or U+FFFD would be
//! data lost without a word.

use encoding_rs::{DecoderResult, Encoder, EncoderResult, Encoding, UTF_8, UTF_16BE, UTF_16LE};
use std::io::{self, Chain, Cursor, Read, Write};

/// How an input turned out to be encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceEncoding {
    pub encoding: &'static Encoding,
    /// Whether the input started with a byte order mark.
    pub bom: bool,
}

/// The longest byte order mark (UTF-8's).
const LONGEST_BOM: usize = 3;

/// A reader with its BOM taken off: the bytes read to look for the mark,
/// put back in front when they were not one, then the rest.
type WithoutBom<R> = Chain<Cursor<Vec<u8>>, R>;

/// An input as UTF-8: passed through when it already is, decoded when not.
pub enum Decoded<R: Read> {
    Utf8(WithoutBom<R>),
    Other(DecodeReader<WithoutBom<R>>),
}

impl<R: Read> Read for Decoded<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Decoded::Utf8(reader) => reader.read(buf),
            Decoded::Other(reader) => reader.read(buf),
        }
    }
}

/// Open `reader` as UTF-8 text: take its BOM off, if any, and decode it
/// from the encoding the BOM names — or, without one, from `declared`.
pub fn open<R: Read>(
    mut reader: R,
    declared: &'static Encoding,
) -> io::Result<(SourceEncoding, Decoded<R>)> {
    let mut head = Vec::with_capacity(LONGEST_BOM);
    //a read may return fewer bytes than asked for, so keep reading until
    //there are enough to decide, or the input ends first
    (&mut reader)
        .take(LONGEST_BOM as u64)
        .read_to_end(&mut head)?;

    let (encoding, bom) = match Encoding::for_bom(&head) {
        Some((encoding, length)) => {
            head.drain(..length);
            (encoding, true)
        }
        None => (declared, false),
    };
    let source = SourceEncoding { encoding, bom };

    let rest = Cursor::new(head).chain(reader);
    let decoded = match encoding == UTF_8 {
        //already what the engine reads: no copy, no check — the engine
        //validates UTF-8 itself, row by row
        true => Decoded::Utf8(rest),
        false => Decoded::Other(DecodeReader::new(rest, encoding)),
    };
    Ok((source, decoded))
}

/// The byte order mark of `encoding`, as written in front of an output;
/// empty for an encoding that has none (every legacy one).
pub fn bom_of(encoding: &'static Encoding) -> &'static [u8] {
    if encoding == UTF_8 {
        b"\xEF\xBB\xBF"
    } else if encoding == UTF_16LE {
        b"\xFF\xFE"
    } else if encoding == UTF_16BE {
        b"\xFE\xFF"
    } else {
        b""
    }
}

/// Whether text can be written in `encoding`. A few encodings exist only
/// to be read: `replacement` decodes anything to one U+FFFD.
pub fn can_encode(encoding: &'static Encoding) -> bool {
    encoding == UTF_16LE || encoding == UTF_16BE || encoding.output_encoding() == encoding
}

/// Bytes read from the input at a time.
const CHUNK: usize = 8 * 1024;

/// A reader decoding another one, in `encoding`, into UTF-8.
pub struct DecodeReader<R: Read> {
    inner: R,
    encoding: &'static Encoding,
    decoder: encoding_rs::Decoder,
    chunk: Vec<u8>,
    //decoded, not yet handed out: `decoded[position..]`
    decoded: Vec<u8>,
    position: usize,
    finished: bool,
}

impl<R: Read> DecodeReader<R> {
    fn new(inner: R, encoding: &'static Encoding) -> Self {
        Self {
            inner,
            encoding,
            //the BOM, if any, is already gone
            decoder: encoding.new_decoder_without_bom_handling(),
            chunk: vec![0; CHUNK],
            decoded: Vec::new(),
            position: 0,
            finished: false,
        }
    }

    /// Decode the next chunk of input. May decode nothing (a chunk ending
    /// inside a character, which the decoder keeps for the next one).
    fn decode_chunk(&mut self) -> io::Result<()> {
        let read = self.inner.read(&mut self.chunk)?;
        let last = read == 0;
        let input = &self.chunk[..read];

        let capacity = self
            .decoder
            .max_utf8_buffer_length_without_replacement(input.len())
            .ok_or_else(|| io::Error::other("input chunk too large to decode"))?;
        self.decoded.resize(capacity, 0);
        let (result, _, written) = self
            .decoder
            .decode_to_utf8_without_replacement(input, &mut self.decoded, last);
        self.decoded.truncate(written);
        self.position = 0;
        self.finished = last;

        match result {
            DecoderResult::InputEmpty => Ok(()),
            //the buffer was sized for the worst case
            DecoderResult::OutputFull => Err(io::Error::other("decoder ran out of room")),
            DecoderResult::Malformed(..) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the input is not valid {}", self.encoding.name()),
            )),
        }
    }
}

impl<R: Read> Read for DecodeReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.position == self.decoded.len() {
            if self.finished {
                return Ok(0);
            }
            self.decode_chunk()?;
        }
        let pending = &self.decoded[self.position..];
        let count = pending.len().min(buf.len());
        buf[..count].copy_from_slice(&pending[..count]);
        self.position += count;
        Ok(count)
    }
}

/// A writer that takes UTF-8 and writes it on in `encoding`. A write may
/// end inside a character (a buffer above splits wherever it is full):
/// the bytes of it seen so far wait for the rest.
pub struct EncodeWriter<W: Write> {
    inner: W,
    encoding: &'static Encoding,
    //`None` for UTF-16, which encoding_rs only decodes
    encoder: Option<Encoder>,
    //the start of a character the last write ended inside
    pending: Vec<u8>,
    encoded: Vec<u8>,
}

impl<W: Write> EncodeWriter<W> {
    /// Encode into `encoding`, which must be one [`can_encode`] accepts.
    pub fn new(inner: W, encoding: &'static Encoding) -> Self {
        let is_utf16 = encoding == UTF_16LE || encoding == UTF_16BE;
        Self {
            inner,
            encoding,
            encoder: (!is_utf16).then(|| encoding.new_encoder()),
            pending: Vec::new(),
            encoded: Vec::new(),
        }
    }

    pub fn get_ref(&self) -> &W {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    fn encode(&mut self, text: &str) -> io::Result<()> {
        self.encoded.clear();
        match &mut self.encoder {
            None => {
                let little_endian = self.encoding == UTF_16LE;
                for unit in text.encode_utf16() {
                    let bytes = match little_endian {
                        true => unit.to_le_bytes(),
                        false => unit.to_be_bytes(),
                    };
                    self.encoded.extend_from_slice(&bytes);
                }
            }
            Some(encoder) => {
                let capacity = encoder
                    .max_buffer_length_from_utf8_without_replacement(text.len())
                    .ok_or_else(|| io::Error::other("output too large to encode"))?;
                self.encoded.resize(capacity, 0);
                let (result, _, written) =
                    encoder.encode_from_utf8_without_replacement(text, &mut self.encoded, false);
                self.encoded.truncate(written);
                match result {
                    EncoderResult::InputEmpty => {}
                    EncoderResult::OutputFull => {
                        return Err(io::Error::other("encoder ran out of room"));
                    }
                    EncoderResult::Unmappable(character) => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "'{character}' (U+{:04X}) cannot be written in {}",
                                u32::from(character),
                                self.encoding.name()
                            ),
                        ));
                    }
                }
            }
        }
        self.inner.write_all(&self.encoded)
    }
}

impl<W: Write> Write for EncodeWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let joined;
        let bytes = if self.pending.is_empty() {
            data
        } else {
            joined = [std::mem::take(&mut self.pending).as_slice(), data].concat();
            &joined
        };

        let complete = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            //a character cut off at the end: encode up to it, keep the rest
            Err(error) if error.error_len().is_none() => {
                let (complete, rest) = bytes.split_at(error.valid_up_to());
                self.pending = rest.to_vec();
                //checked just now: everything up to there is valid
                std::str::from_utf8(complete).map_err(io::Error::other)?
            }
            Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
        };
        self.encode(complete)?;
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding_rs::{SHIFT_JIS, WINDOWS_1250};

    fn opened(input: &[u8], declared: &'static Encoding) -> io::Result<(SourceEncoding, Vec<u8>)> {
        let (source, mut reader) = open(input, declared)?;
        let mut text = Vec::new();
        reader.read_to_end(&mut text)?;
        Ok((source, text))
    }

    fn utf8(input: &[u8]) -> (bool, Vec<u8>) {
        let (source, text) = opened(input, UTF_8).unwrap();
        assert_eq!(source.encoding, UTF_8);
        (source.bom, text)
    }

    fn encoded(encoding: &'static Encoding, writes: &[&[u8]]) -> io::Result<Vec<u8>> {
        let mut writer = EncodeWriter::new(Vec::new(), encoding);
        for data in writes {
            writer.write_all(data)?;
        }
        Ok(writer.inner)
    }

    /// Hands out one byte per read, as a pipe may.
    struct Trickle<'a>(&'a [u8]);

    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match (self.0.split_first(), buf.first_mut()) {
                (Some((byte, rest)), Some(slot)) => {
                    *slot = *byte;
                    self.0 = rest;
                    Ok(1)
                }
                _ => Ok(0),
            }
        }
    }

    #[test]
    fn a_leading_utf8_bom_is_taken_off() {
        // Given / When
        let (had_bom, rest) = utf8(b"\xEF\xBB\xBFa,b\n");

        // Then
        assert!(had_bom);
        assert_eq!(rest, b"a,b\n");
    }

    #[test]
    fn input_without_a_bom_is_left_whole() {
        // Given / When / Then
        assert_eq!(utf8(b"a,b\n"), (false, b"a,b\n".to_vec()));
        assert_eq!(utf8(b"ab"), (false, b"ab".to_vec()));
        assert_eq!(utf8(b""), (false, Vec::new()));
    }

    #[test]
    fn a_bom_prefix_alone_is_not_a_bom() {
        // Given only the first two bytes of the mark
        // When / Then they are kept: they are data, however odd
        assert_eq!(utf8(b"\xEF\xBB"), (false, b"\xEF\xBB".to_vec()));
    }

    #[test]
    fn only_a_bom_at_the_very_start_counts() {
        // Given the mark further into the input
        // When / Then it is content, not an encoding mark
        let input = b"a\xEF\xBB\xBF\n";
        assert_eq!(utf8(input), (false, input.to_vec()));
    }

    #[test]
    fn a_bom_split_across_reads_is_still_found() {
        // Given a reader returning one byte at a time
        let (source, mut reader) = open(Trickle(b"\xEF\xBB\xBFx"), UTF_8).unwrap();

        // When
        let mut rest = Vec::new();
        reader.read_to_end(&mut rest).unwrap();

        // Then
        assert!(source.bom);
        assert_eq!(rest, b"x");
    }

    #[test]
    fn a_utf16_bom_names_the_encoding_whatever_was_declared() {
        // Given "zażółć\n" in UTF-16LE and UTF-16BE, each with its BOM
        let text = "zażółć\n";
        let le: Vec<u8> = b"\xFF\xFE"
            .iter()
            .copied()
            .chain(
                text.encode_utf16()
                    .flat_map(u16::to_le_bytes),
            )
            .collect();
        let be: Vec<u8> = b"\xFE\xFF"
            .iter()
            .copied()
            .chain(
                text.encode_utf16()
                    .flat_map(u16::to_be_bytes),
            )
            .collect();

        // When
        let (le_source, le_text) = opened(&le, WINDOWS_1250).unwrap();
        let (be_source, be_text) = opened(&be, UTF_8).unwrap();

        // Then
        assert_eq!((le_source.encoding, le_source.bom), (UTF_16LE, true));
        assert_eq!((be_source.encoding, be_source.bom), (UTF_16BE, true));
        assert_eq!(le_text, text.as_bytes());
        assert_eq!(be_text, text.as_bytes());
    }

    #[test]
    fn a_declared_legacy_encoding_is_decoded() {
        // Given "Łódź" in windows-1250
        let input = b"\xA3\xF3d\x9F\n";

        // When
        let (source, text) = opened(input, WINDOWS_1250).unwrap();

        // Then
        assert_eq!((source.encoding, source.bom), (WINDOWS_1250, false));
        assert_eq!(text, "Łódź\n".as_bytes());
    }

    #[test]
    fn decoding_spans_chunks() {
        // Given more than one chunk of a two-byte encoding, read a byte
        // at a time, so characters are cut at every possible point
        let text = "日本語のテキスト\n".repeat(CHUNK / 8);
        let (bytes, _, _) = SHIFT_JIS.encode(&text);
        let (_, mut reader) = open(Trickle(&bytes), SHIFT_JIS).unwrap();

        // When
        let mut decoded = Vec::new();
        reader
            .read_to_end(&mut decoded)
            .unwrap();

        // Then
        assert_eq!(decoded, text.as_bytes());
    }

    #[test]
    fn malformed_input_is_an_error_not_a_replacement() {
        // Given an unpaired surrogate in UTF-16LE
        let input = b"\xFF\xFEa\x00\x00\xD8b\x00";

        // When
        let error = opened(input, UTF_8).unwrap_err();

        // Then
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(
            error
                .to_string()
                .contains("not valid UTF-16LE"),
            "{error}"
        );
    }

    #[test]
    fn output_is_encoded_into_a_legacy_encoding() {
        // Given / When
        let bytes = encoded(WINDOWS_1250, &["Łódź\n".as_bytes()]).unwrap();

        // Then
        assert_eq!(bytes, b"\xA3\xF3d\x9F\n");
    }

    #[test]
    fn output_is_encoded_into_utf16() {
        // Given / When
        let le = encoded(UTF_16LE, &["aż".as_bytes()]).unwrap();
        let be = encoded(UTF_16BE, &["aż".as_bytes()]).unwrap();

        // Then
        assert_eq!(le, b"a\x00\x7C\x01");
        assert_eq!(be, b"\x00a\x01\x7C");
    }

    #[test]
    fn a_character_split_across_writes_is_encoded_whole() {
        // Given "ż" (C5 BC) cut in two by the writes
        // When
        let bytes = encoded(WINDOWS_1250, &[b"a\xC5", b"\xBCb"]).unwrap();

        // Then
        assert_eq!(bytes, b"a\xBFb");
    }

    #[test]
    fn an_unmappable_character_is_an_error() {
        // Given a character windows-1250 has no byte for
        // When
        let error = encoded(WINDOWS_1250, &["a€b日".as_bytes()]).unwrap_err();

        // Then
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(
            error
                .to_string()
                .contains("'日' (U+65E5) cannot be written in windows-1250"),
            "{error}"
        );
    }

    #[test]
    fn decode_then_encode_round_trips() {
        // Given windows-1250 bytes covering its whole upper half that maps
        let original: Vec<u8> = (0x20..=0xFF)
            .filter(|&byte| {
                let bytes = [byte];
                let (text, _, malformed) = WINDOWS_1250.decode(&bytes);
                !malformed && !text.is_empty()
            })
            .collect();

        // When
        let (_, text) = opened(&original, WINDOWS_1250).unwrap();
        let bytes = encoded(WINDOWS_1250, &[&text]).unwrap();

        // Then
        assert_eq!(bytes, original);
    }

    #[test]
    fn boms_and_writability() {
        // Given / When / Then
        assert_eq!(bom_of(UTF_8), b"\xEF\xBB\xBF");
        assert_eq!(bom_of(UTF_16LE), b"\xFF\xFE");
        assert_eq!(bom_of(WINDOWS_1250), b"");
        assert!(can_encode(UTF_16BE));
        assert!(can_encode(WINDOWS_1250));
        assert!(!can_encode(encoding_rs::REPLACEMENT));
    }
}
