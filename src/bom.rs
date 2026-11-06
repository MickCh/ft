//! The UTF-8 byte order mark. Some editors (Notepad, Excel's CSV export)
//! start a file with `EF BB BF`; read as text it is an invisible U+FEFF
//! glued to the first line, which then counts as an extra column, makes
//! the first field fail to match `^…$` and the first number fail to
//! parse. It says how the file is encoded, not what it holds, so it is
//! taken off before processing — per input file, since every file of a
//! concatenation may carry one — and an in-place edit puts it back.

use std::io::{self, Chain, Cursor, Read};

/// The UTF-8 encoding of U+FEFF.
pub const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// A reader with its BOM taken off: the bytes read to look for the mark,
/// put back in front when they were not one, then the rest.
pub type WithoutBom<R> = Chain<Cursor<Vec<u8>>, R>;

/// Take a leading UTF-8 BOM off `reader`. Returns whether there was one,
/// and a reader that yields everything after it.
pub fn strip_bom<R: Read>(mut reader: R) -> io::Result<(bool, WithoutBom<R>)> {
    let mut head = Vec::with_capacity(UTF8_BOM.len());
    //a read may return fewer bytes than asked for, so keep reading until
    //there are enough to decide, or the input ends first
    (&mut reader)
        .take(UTF8_BOM.len() as u64)
        .read_to_end(&mut head)?;

    let had_bom = head == UTF8_BOM;
    if had_bom {
        head.clear();
    }
    Ok((had_bom, Cursor::new(head).chain(reader)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stripped(input: &[u8]) -> (bool, Vec<u8>) {
        let (had_bom, mut reader) = strip_bom(input).unwrap();
        let mut rest = Vec::new();
        reader.read_to_end(&mut rest).unwrap();
        (had_bom, rest)
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
    fn a_leading_bom_is_taken_off() {
        // Given / When
        let (had_bom, rest) = stripped(b"\xEF\xBB\xBFa,b\n");

        // Then
        assert!(had_bom);
        assert_eq!(rest, b"a,b\n");
    }

    #[test]
    fn input_without_a_bom_is_left_whole() {
        // Given / When / Then
        assert_eq!(stripped(b"a,b\n"), (false, b"a,b\n".to_vec()));
        assert_eq!(stripped(b"ab"), (false, b"ab".to_vec()));
        assert_eq!(stripped(b""), (false, Vec::new()));
    }

    #[test]
    fn a_bom_prefix_alone_is_not_a_bom() {
        // Given only the first two bytes of the mark
        // When / Then they are kept: they are data, however odd
        assert_eq!(stripped(b"\xEF\xBB"), (false, b"\xEF\xBB".to_vec()));
    }

    #[test]
    fn only_a_bom_at_the_very_start_counts() {
        // Given the mark further into the input
        // When / Then it is content, not an encoding mark
        let input = b"a\xEF\xBB\xBF\n";
        assert_eq!(stripped(input), (false, input.to_vec()));
    }

    #[test]
    fn a_bom_split_across_reads_is_still_found() {
        // Given a reader returning one byte at a time
        let (had_bom, mut reader) = strip_bom(Trickle(b"\xEF\xBB\xBFx")).unwrap();

        // When
        let mut rest = Vec::new();
        reader.read_to_end(&mut rest).unwrap();

        // Then
        assert!(had_bom);
        assert_eq!(rest, b"x");
    }
}
