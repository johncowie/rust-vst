//! Key and scale → the diatonic third above a note.

use crate::grid::{Grid, PITCH_CLASSES, Row};
use crate::note::NAMES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Major,
    NaturalMinor,
}

/// Semitones above the key of each scale degree.
const MAJOR_STEPS: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const NATURAL_MINOR_STEPS: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];

/// Semitones from each pitch class (relative to the key) up to the
/// diatonic third of its scale degree. A pitch class outside the scale
/// uses the scale note a semitone below: in both scales every chromatic
/// note sits between two scale notes, so "nearest" is always a tie.
const fn thirds(steps: [u8; 7]) -> [i32; 12] {
    let mut table = [0; 12];
    let mut relative = 0;
    while relative < 12 {
        // The degree on this pitch class, else the one a semitone below.
        // One of the two always exists: 0 is in every scale, and no two
        // chromatic notes are adjacent.
        let mut degree = 7;
        let mut d = 0;
        while d < 7 {
            if steps[d] == relative {
                degree = d;
            }
            d += 1;
        }
        if degree == 7 {
            d = 0;
            while d < 7 {
                if steps[d] + 1 == relative {
                    degree = d;
                }
                d += 1;
            }
        }
        assert!(degree < 7, "a pitch class with no scale note below it");
        let third = steps[(degree + 2) % 7];
        table[relative as usize] = ((third + 12 - steps[degree]) % 12) as i32;
        relative += 1;
    }
    table
}

const MAJOR_THIRDS: [i32; 12] = thirds(MAJOR_STEPS);
const NATURAL_MINOR_THIRDS: [i32; 12] = thirds(NATURAL_MINOR_STEPS);

/// `key` is a pitch class, 0 = C … 11 = B. `midi` may be any note; the
/// octave is ignored.
pub fn third_above(key: u8, scale: Scale, midi: i32) -> i32 {
    let relative = (midi - i32::from(key)).rem_euclid(12) as usize;
    match scale {
        Scale::Major => MAJOR_THIRDS[relative],
        Scale::NaturalMinor => NATURAL_MINOR_THIRDS[relative],
    }
}

/// The grid whose row for each pitch class is the diatonic third above
/// it. A bridge for the plugin and harness until Phases 5 and 6 of plan
/// 0007 replace Key and Scale.
pub fn diatonic_grid(key: u8, scale: Scale) -> Grid {
    let mut grid = Grid::EMPTY;
    for pc in 0..PITCH_CLASSES {
        let third = third_above(key, scale, pc as i32) as i8;
        grid.set_row(pc, Row::EMPTY.with(third, true));
    }
    grid
}

/// "C" … "B" (sharps), for the harness summary and CSV.
pub fn format_key(key: u8) -> &'static str {
    NAMES[usize::from(key % 12)]
}

/// Case-insensitive "C", "c#", "Db", "Bb", "E#"…; `None` otherwise.
pub fn parse_key(text: &str) -> Option<u8> {
    let mut chars = text.chars();
    let natural: i32 = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let accidental = match chars.next() {
        None => 0,
        Some('#') => 1,
        Some('b' | 'B') => -1,
        Some(_) => return None,
    };
    if chars.next().is_some() {
        return None;
    }
    Some((natural + accidental).rem_euclid(12) as u8)
}

/// "major"; "minor", "natural-minor" or "natural minor" (case-insensitive).
pub fn parse_scale(text: &str) -> Option<Scale> {
    match text.to_ascii_lowercase().as_str() {
        "major" => Some(Scale::Major),
        "minor" | "natural-minor" | "natural minor" => Some(Scale::NaturalMinor),
        _ => None,
    }
}

pub fn format_scale(scale: Scale) -> &'static str {
    match scale {
        Scale::Major => "major",
        Scale::NaturalMinor => "natural minor",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCALES: [Scale; 2] = [Scale::Major, Scale::NaturalMinor];

    /// Typed from the degree intervals, not from `thirds`:
    /// - major: I +4, ii +3, iii +3, IV +4, V +4, vi +3, vii +3;
    /// - natural minor: i +3, ii +3, III +4, iv +3, v +3, VI +4, VII +4.
    ///
    /// Each chromatic pitch class copies the scale note a semitone below.
    const REFERENCE: [[i32; 12]; 2] = [
        // C  C# D  D# E  F  F# G  G# A  A# B   (relative to the key)
        [4, 4, 3, 3, 3, 4, 4, 4, 4, 3, 3, 3],
        // 1  -  2  3  -  4  -  5  6  -  7  -
        [3, 3, 3, 4, 4, 3, 3, 3, 4, 4, 4, 4],
    ];

    #[test]
    fn every_key_scale_and_pitch_class_matches_the_reference() {
        for key in 0..12u8 {
            for (s, scale) in SCALES.into_iter().enumerate() {
                for pc in 0..12 {
                    let expected = REFERENCE[s][(pc - i32::from(key)).rem_euclid(12) as usize];
                    for octave in [3, 4, 5] {
                        let midi = 12 * (octave + 1) + pc;
                        assert_eq!(
                            third_above(key, scale, midi),
                            expected,
                            "key {key} {scale:?} midi {midi}"
                        );
                    }
                }
            }
        }
    }

    /// (sung note, harmony note) pairs, by name.
    fn check(key: &str, scale: Scale, pairs: &[(&str, &str)]) {
        let key = parse_key(key).unwrap();
        for &(sung, harmony) in pairs {
            let sung_pc = i32::from(parse_key(sung).unwrap());
            let harmony_pc = i32::from(parse_key(harmony).unwrap());
            let midi = 60 + sung_pc;
            let got = (midi + third_above(key, scale, midi)).rem_euclid(12);
            assert_eq!(got, harmony_pc, "{sung} → {harmony}");
        }
    }

    #[test]
    fn c_major_gets_its_diatonic_thirds() {
        check(
            "C",
            Scale::Major,
            &[
                ("C", "E"),
                ("D", "F"),
                ("E", "G"),
                ("F", "A"),
                ("G", "B"),
                ("A", "C"),
                ("B", "D"),
            ],
        );
    }

    #[test]
    fn a_minor_gets_its_diatonic_thirds() {
        check(
            "A",
            Scale::NaturalMinor,
            &[
                ("A", "C"),
                ("B", "D"),
                ("C", "E"),
                ("D", "F"),
                ("E", "G"),
                ("F", "A"),
                ("G", "B"),
            ],
        );
    }

    #[test]
    fn out_of_key_notes_in_c_major_use_the_note_below() {
        check(
            "C",
            Scale::Major,
            &[
                ("C#", "F"),
                ("Eb", "F#"),
                ("F#", "A#"),
                ("G#", "C"),
                ("Bb", "C#"),
            ],
        );
    }

    #[test]
    fn negative_midi_wraps_by_pitch_class() {
        // -12 and -1 are C and B, octaves below MIDI 0.
        assert_eq!(third_above(0, Scale::Major, -12), 4);
        assert_eq!(third_above(0, Scale::Major, -1), 3);
        assert_eq!(third_above(9, Scale::NaturalMinor, -3), 3);
    }

    #[test]
    fn diatonic_grid_rows_hold_the_third() {
        let grid = diatonic_grid(0, Scale::Major);
        assert_eq!(grid.row(2).intervals().collect::<Vec<_>>(), [3]);
        assert_eq!(grid.row(0).intervals().collect::<Vec<_>>(), [4]);
    }

    #[test]
    fn keys_round_trip() {
        for key in 0..12 {
            assert_eq!(parse_key(format_key(key)), Some(key));
        }
    }

    #[test]
    fn keys_accept_flats_and_lower_case() {
        assert_eq!(parse_key("c"), Some(0));
        assert_eq!(parse_key("c#"), Some(1));
        assert_eq!(parse_key("Db"), Some(1));
        assert_eq!(parse_key("db"), Some(1));
        assert_eq!(parse_key("Bb"), Some(10));
        assert_eq!(parse_key("a#"), Some(10));
        assert_eq!(parse_key("E#"), Some(5));
        assert_eq!(parse_key("Cb"), Some(11));
    }

    #[test]
    fn bad_keys_are_rejected() {
        for text in ["H", "", "C##", "Cb5", "#", "C ", "major"] {
            assert_eq!(parse_key(text), None, "{text:?}");
        }
    }

    #[test]
    fn scales_parse() {
        assert_eq!(parse_scale("major"), Some(Scale::Major));
        assert_eq!(parse_scale("Major"), Some(Scale::Major));
        for text in ["minor", "natural-minor", "natural minor", "Natural Minor"] {
            assert_eq!(parse_scale(text), Some(Scale::NaturalMinor), "{text}");
        }
        for text in ["dorian", "", "minor ", "harmonic minor"] {
            assert_eq!(parse_scale(text), None, "{text:?}");
        }
        for scale in SCALES {
            assert_eq!(parse_scale(format_scale(scale)), Some(scale));
        }
    }
}
