use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Normalizacja do dopasowywania reguł: lowercase, bez diakrytyków, tylko
/// alfanumeryka i pojedyncze spacje. Nie służy do wyświetlania.
pub fn normalize_for_rules(input: &str) -> String {
    let lowered = input.to_lowercase();
    let decomposed: String = lowered.nfkd().filter(|c| !is_combining_mark(*c)).collect();
    let mapped: String = decomposed
        .chars()
        .map(|c| match c {
            'ł' => 'l',
            'Ł' => 'l',
            _ => c,
        })
        .collect();

    let mut out = String::with_capacity(mapped.len());
    let mut pending_space = false;
    for c in mapped.chars() {
        if c.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            out.push(c);
            pending_space = false;
        } else {
            pending_space = true;
        }
    }
    out
}

/// Wariant antyspoofingowy: cyfry udające litery (`m13szka` → `mieszka`).
pub fn deleet(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            '0' => 'o',
            '1' => 'i',
            '3' => 'e',
            '4' => 'a',
            '5' => 's',
            '7' => 't',
            _ => c,
        })
        .collect()
}

/// Normalizacja do porównywania cytatów: lowercase, zachowane diakrytyki,
/// interpunkcja jako spacje, kolaps whitespace.
pub fn normalize_for_match(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for c in input.to_lowercase().chars() {
        let c = match c {
            '„' | '”' | '“' | '’' | '‘' | '«' | '»' => '\'',
            '\u{00a0}' => ' ',
            _ => c,
        };
        if c.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            out.push(c);
            pending_space = false;
        } else {
            pending_space = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polish_diacritics_removed() {
        assert_eq!(
            normalize_for_rules("Gdzie MIESZKA Łukasz?"),
            "gdzie mieszka lukasz"
        );
        assert_eq!(normalize_for_rules("ŻÓŁĆ"), "zolc");
    }

    #[test]
    fn leet_variant() {
        assert_eq!(normalize_for_rules("gdz13 m13szka"), "gdz13 m13szka");
        assert_eq!(deleet("gdz13 m13szka"), "gdzie mieszka");
    }

    #[test]
    fn match_keeps_diacritics() {
        assert_eq!(
            normalize_for_match("  To   NIE jest „prawdą”! "),
            "to nie jest prawdą"
        );
    }
}
