use super::{Query, Target, flags};

#[test]
fn prepared_ascii_and_unicode_targets_keep_exact_utf16_membership() {
    for (input, normalized) in [
        (
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ 0127",
            "abcdefghijklmnopqrstuvwxyz 0127",
        ),
        ("CAFÉ ÁÅİ Ω𝔸 a", "cafe aai ω𝔸 a"),
        (
            "ΩΩΩ abcdefghijklmnopqrstuvwxyz",
            "ωωω abcdefghijklmnopqrstuvwxyz",
        ),
        ("a\u{301} É\u{301} 漢字 😀", "a e 漢字 😀"),
        ("", ""),
    ] {
        let expected: Vec<_> = normalized.encode_utf16().collect();
        for target in [Target::new(input), Target::membership(input)] {
            assert_eq!(target.codes, expected, "{input:?}");
            assert_eq!(target.flags, flags(&expected));
            for code in expected.iter().copied().chain([0, 127, 0xffff]) {
                for start in 0..=expected.len() {
                    assert_eq!(
                        target.after(code, start),
                        expected
                            .iter()
                            .enumerate()
                            .skip(start)
                            .find(|(_, actual)| **actual == code)
                            .map(|(i, _)| i + 1),
                        "{input:?} code={code} start={start}",
                    );
                }
            }
        }
        for query in ["a", "abc", "cafe a", "ω a", "𝔸", "漢 😀", "not present"] {
            let compiled = Query::new(query);
            assert_eq!(
                compiled.matches(&[Target::membership(input)]),
                compiled.score(&[Target::new(input)], false).is_some(),
                "{input:?} query={query:?}",
            );
        }
    }
}
