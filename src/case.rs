//! Titlecase, which Rust's standard library does not have: for most
//! chars it is the uppercase, but a ligature or a digraph capitalizes
//! only its first letter (`ﬁ` → `Fi`, `ǆ` → `ǅ`, `ß` → `Ss`), where
//! uppercasing would capitalize all of it (`FI`, `Ǆ`, `SS`).

/// Write the titlecase of `character` into `out`.
pub fn push_titlecase(character: char, out: &mut String) {
    match TITLECASE.binary_search_by_key(&character, |&(from, _)| from) {
        Ok(index) => out.push_str(TITLECASE[index].1),
        Err(_) => out.extend(character.to_uppercase()),
    }
}

// Generated from Unicode 15.0.0 (Python's str.title): every char whose
// titlecase differs from its uppercase, with its titlecase, by char.
const TITLECASE: &[(char, &str)] = &[
    ('\u{df}', "\u{53}\u{73}"),
    ('\u{1c4}', "\u{1c5}"),
    ('\u{1c5}', "\u{1c5}"),
    ('\u{1c6}', "\u{1c5}"),
    ('\u{1c7}', "\u{1c8}"),
    ('\u{1c8}', "\u{1c8}"),
    ('\u{1c9}', "\u{1c8}"),
    ('\u{1ca}', "\u{1cb}"),
    ('\u{1cb}', "\u{1cb}"),
    ('\u{1cc}', "\u{1cb}"),
    ('\u{1f1}', "\u{1f2}"),
    ('\u{1f2}', "\u{1f2}"),
    ('\u{1f3}', "\u{1f2}"),
    ('\u{587}', "\u{535}\u{582}"),
    ('\u{10d0}', "\u{10d0}"),
    ('\u{10d1}', "\u{10d1}"),
    ('\u{10d2}', "\u{10d2}"),
    ('\u{10d3}', "\u{10d3}"),
    ('\u{10d4}', "\u{10d4}"),
    ('\u{10d5}', "\u{10d5}"),
    ('\u{10d6}', "\u{10d6}"),
    ('\u{10d7}', "\u{10d7}"),
    ('\u{10d8}', "\u{10d8}"),
    ('\u{10d9}', "\u{10d9}"),
    ('\u{10da}', "\u{10da}"),
    ('\u{10db}', "\u{10db}"),
    ('\u{10dc}', "\u{10dc}"),
    ('\u{10dd}', "\u{10dd}"),
    ('\u{10de}', "\u{10de}"),
    ('\u{10df}', "\u{10df}"),
    ('\u{10e0}', "\u{10e0}"),
    ('\u{10e1}', "\u{10e1}"),
    ('\u{10e2}', "\u{10e2}"),
    ('\u{10e3}', "\u{10e3}"),
    ('\u{10e4}', "\u{10e4}"),
    ('\u{10e5}', "\u{10e5}"),
    ('\u{10e6}', "\u{10e6}"),
    ('\u{10e7}', "\u{10e7}"),
    ('\u{10e8}', "\u{10e8}"),
    ('\u{10e9}', "\u{10e9}"),
    ('\u{10ea}', "\u{10ea}"),
    ('\u{10eb}', "\u{10eb}"),
    ('\u{10ec}', "\u{10ec}"),
    ('\u{10ed}', "\u{10ed}"),
    ('\u{10ee}', "\u{10ee}"),
    ('\u{10ef}', "\u{10ef}"),
    ('\u{10f0}', "\u{10f0}"),
    ('\u{10f1}', "\u{10f1}"),
    ('\u{10f2}', "\u{10f2}"),
    ('\u{10f3}', "\u{10f3}"),
    ('\u{10f4}', "\u{10f4}"),
    ('\u{10f5}', "\u{10f5}"),
    ('\u{10f6}', "\u{10f6}"),
    ('\u{10f7}', "\u{10f7}"),
    ('\u{10f8}', "\u{10f8}"),
    ('\u{10f9}', "\u{10f9}"),
    ('\u{10fa}', "\u{10fa}"),
    ('\u{10fd}', "\u{10fd}"),
    ('\u{10fe}', "\u{10fe}"),
    ('\u{10ff}', "\u{10ff}"),
    ('\u{1f80}', "\u{1f88}"),
    ('\u{1f81}', "\u{1f89}"),
    ('\u{1f82}', "\u{1f8a}"),
    ('\u{1f83}', "\u{1f8b}"),
    ('\u{1f84}', "\u{1f8c}"),
    ('\u{1f85}', "\u{1f8d}"),
    ('\u{1f86}', "\u{1f8e}"),
    ('\u{1f87}', "\u{1f8f}"),
    ('\u{1f88}', "\u{1f88}"),
    ('\u{1f89}', "\u{1f89}"),
    ('\u{1f8a}', "\u{1f8a}"),
    ('\u{1f8b}', "\u{1f8b}"),
    ('\u{1f8c}', "\u{1f8c}"),
    ('\u{1f8d}', "\u{1f8d}"),
    ('\u{1f8e}', "\u{1f8e}"),
    ('\u{1f8f}', "\u{1f8f}"),
    ('\u{1f90}', "\u{1f98}"),
    ('\u{1f91}', "\u{1f99}"),
    ('\u{1f92}', "\u{1f9a}"),
    ('\u{1f93}', "\u{1f9b}"),
    ('\u{1f94}', "\u{1f9c}"),
    ('\u{1f95}', "\u{1f9d}"),
    ('\u{1f96}', "\u{1f9e}"),
    ('\u{1f97}', "\u{1f9f}"),
    ('\u{1f98}', "\u{1f98}"),
    ('\u{1f99}', "\u{1f99}"),
    ('\u{1f9a}', "\u{1f9a}"),
    ('\u{1f9b}', "\u{1f9b}"),
    ('\u{1f9c}', "\u{1f9c}"),
    ('\u{1f9d}', "\u{1f9d}"),
    ('\u{1f9e}', "\u{1f9e}"),
    ('\u{1f9f}', "\u{1f9f}"),
    ('\u{1fa0}', "\u{1fa8}"),
    ('\u{1fa1}', "\u{1fa9}"),
    ('\u{1fa2}', "\u{1faa}"),
    ('\u{1fa3}', "\u{1fab}"),
    ('\u{1fa4}', "\u{1fac}"),
    ('\u{1fa5}', "\u{1fad}"),
    ('\u{1fa6}', "\u{1fae}"),
    ('\u{1fa7}', "\u{1faf}"),
    ('\u{1fa8}', "\u{1fa8}"),
    ('\u{1fa9}', "\u{1fa9}"),
    ('\u{1faa}', "\u{1faa}"),
    ('\u{1fab}', "\u{1fab}"),
    ('\u{1fac}', "\u{1fac}"),
    ('\u{1fad}', "\u{1fad}"),
    ('\u{1fae}', "\u{1fae}"),
    ('\u{1faf}', "\u{1faf}"),
    ('\u{1fb2}', "\u{1fba}\u{345}"),
    ('\u{1fb3}', "\u{1fbc}"),
    ('\u{1fb4}', "\u{386}\u{345}"),
    ('\u{1fb7}', "\u{391}\u{342}\u{345}"),
    ('\u{1fbc}', "\u{1fbc}"),
    ('\u{1fc2}', "\u{1fca}\u{345}"),
    ('\u{1fc3}', "\u{1fcc}"),
    ('\u{1fc4}', "\u{389}\u{345}"),
    ('\u{1fc7}', "\u{397}\u{342}\u{345}"),
    ('\u{1fcc}', "\u{1fcc}"),
    ('\u{1ff2}', "\u{1ffa}\u{345}"),
    ('\u{1ff3}', "\u{1ffc}"),
    ('\u{1ff4}', "\u{38f}\u{345}"),
    ('\u{1ff7}', "\u{3a9}\u{342}\u{345}"),
    ('\u{1ffc}', "\u{1ffc}"),
    ('\u{fb00}', "\u{46}\u{66}"),
    ('\u{fb01}', "\u{46}\u{69}"),
    ('\u{fb02}', "\u{46}\u{6c}"),
    ('\u{fb03}', "\u{46}\u{66}\u{69}"),
    ('\u{fb04}', "\u{46}\u{66}\u{6c}"),
    ('\u{fb05}', "\u{53}\u{74}"),
    ('\u{fb06}', "\u{53}\u{74}"),
    ('\u{fb13}', "\u{544}\u{576}"),
    ('\u{fb14}', "\u{544}\u{565}"),
    ('\u{fb15}', "\u{544}\u{56b}"),
    ('\u{fb16}', "\u{54e}\u{576}"),
    ('\u{fb17}', "\u{544}\u{56d}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn titlecased(character: char) -> String {
        let mut out = String::new();
        push_titlecase(character, &mut out);
        out
    }

    #[test]
    fn titlecase_capitalizes_only_the_first_letter_of_a_ligature() {
        // Given ligatures, digraphs and ß / When titlecased / Then only
        // their first letter is capital
        assert_eq!(titlecased('\u{fb01}'), "Fi");
        assert_eq!(titlecased('\u{1c6}'), "\u{1c5}");
        assert_eq!(titlecased('\u{1c4}'), "\u{1c5}");
        assert_eq!(titlecased('\u{df}'), "Ss");

        // And everything else is its uppercase
        assert_eq!(titlecased('a'), "A");
        assert_eq!(titlecased('\u{17c}'), "\u{17b}");
    }

    #[test]
    fn the_table_is_sorted_for_the_binary_search() {
        assert!(
            TITLECASE
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0)
        );
    }
}
