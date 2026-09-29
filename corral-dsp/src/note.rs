//! Frequency → nearest MIDI note and cents offset (A4 = 440 Hz), and display.

pub(crate) const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A note and a whole-cent offset in −50..=49. A frequency exactly 50 cents
/// above a note rounds up to the next note at −50, so the two edges never
/// disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    pub midi: i32,
    pub cents: i32,
}

/// `None` for zero, negative or non-finite input.
pub fn nearest_note(f0_hz: f32) -> Option<Note> {
    if !(f0_hz.is_finite() && f0_hz > 0.0) {
        return None;
    }
    // Round to whole cents first, then split with integer maths. The float
    // error from an exact ±50-cent input (≪ 0.5 cent) can't change the result.
    let cents_from_a4 = (1200.0 * (f64::from(f0_hz) / 440.0).log2()).round() as i32;
    let shifted = cents_from_a4 + 69 * 100 + 50;
    Some(Note {
        midi: shifted.div_euclid(100),
        cents: shifted.rem_euclid(100) - 50,
    })
}

/// The fractional MIDI note of a frequency: 69.0 at 440 Hz, 69.5 a
/// quarter tone above.
pub fn midi_from_hz(f0_hz: f32) -> f32 {
    (69.0 + 12.0 * (f64::from(f0_hz) / 440.0).log2()) as f32
}

/// e.g. "D4", "C#3". MIDI 60 is C4.
pub fn format_name(midi: i32) -> String {
    let name = NAMES[midi.rem_euclid(12) as usize];
    let octave = midi.div_euclid(12) - 1;
    format!("{name}{octave}")
}

/// e.g. "A4 +0¢", "D4 +12¢", "C#3 -7¢".
pub fn format_note(note: Note) -> String {
    format!("{} {:+}¢", format_name(note.midi), note.cents)
}

/// Packed "no note", for the audio → editor atomic.
pub const UNVOICED: u32 = u32::MAX;

/// Packs a note (or unvoiced) into one `u32`, so the audio thread can
/// publish it with a single store and compare it cheaply.
pub fn pack(note: Option<Note>) -> u32 {
    match note {
        None => UNVOICED,
        Some(Note { midi, cents }) => (u32::from(midi as i16 as u16) << 16) | (cents + 50) as u32,
    }
}

pub fn unpack(bits: u32) -> Option<Note> {
    (bits != UNVOICED).then(|| Note {
        midi: i32::from((bits >> 16) as u16 as i16),
        cents: (bits & 0xFFFF) as i32 - 50,
    })
}

/// Packs a MIDI note (or none) into one `u32` for the audio → editor
/// atomic. MIDI −1 would collide with `UNVOICED`, but notes are never
/// negative here.
pub fn pack_midi(midi: Option<i32>) -> u32 {
    midi.map_or(UNVOICED, |m| m as u32)
}

pub fn unpack_midi(bits: u32) -> Option<i32> {
    (bits != UNVOICED).then_some(bits as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Equal-tempered frequency of a MIDI note, offset by `cents`.
    fn freq(midi: i32, cents: f64) -> f32 {
        (440.0 * 2f64.powf((f64::from(midi) - 69.0 + cents / 100.0) / 12.0)) as f32
    }

    fn note(midi: i32, cents: i32) -> Note {
        Note { midi, cents }
    }

    #[test]
    fn a440_is_a4_with_no_offset() {
        let a4 = nearest_note(440.0).unwrap();
        assert_eq!(a4, note(69, 0));
        assert_eq!(format_note(a4), "A4 +0¢");
    }

    #[test]
    fn every_note_from_c1_to_b7_maps_to_itself() {
        for midi in 24..=107 {
            assert_eq!(
                nearest_note(freq(midi, 0.0)),
                Some(note(midi, 0)),
                "MIDI {midi}"
            );
        }
    }

    #[test]
    fn offsets_within_the_note_keep_the_note() {
        for midi in 24..=107 {
            for cents in [-49, -10, 10, 49] {
                assert_eq!(
                    nearest_note(freq(midi, f64::from(cents))),
                    Some(note(midi, cents)),
                    "MIDI {midi} {cents:+} cents"
                );
            }
        }
    }

    #[test]
    fn exactly_fifty_cents_up_is_the_next_note_at_minus_fifty() {
        for midi in 24..=107 {
            assert_eq!(
                nearest_note(freq(midi, 50.0)),
                Some(note(midi + 1, -50)),
                "MIDI {midi}"
            );
        }
    }

    #[test]
    fn exactly_fifty_cents_down_is_the_same_note_at_minus_fifty() {
        for midi in 24..=107 {
            assert_eq!(
                nearest_note(freq(midi, -50.0)),
                Some(note(midi, -50)),
                "MIDI {midi}"
            );
        }
    }

    #[test]
    fn formats_octave_boundaries_and_sharps() {
        assert_eq!(format_note(note(59, 3)), "B3 +3¢");
        assert_eq!(format_note(note(60, 0)), "C4 +0¢");
        assert_eq!(format_note(note(49, -7)), "C#3 -7¢");
        assert_eq!(format_note(note(62, 12)), "D4 +12¢");
    }

    #[test]
    fn invalid_frequencies_have_no_note() {
        for f0 in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(nearest_note(f0), None, "{f0}");
        }
    }

    #[test]
    fn pack_round_trips_across_the_detection_range() {
        // E2 (40) is the first note above 80 Hz; B5 (83) the last below 1 kHz.
        for midi in 40..=83 {
            for cents in -50..=49 {
                let n = note(midi, cents);
                let packed = pack(Some(n));
                assert_ne!(packed, UNVOICED);
                assert_eq!(unpack(packed), Some(n));
            }
        }
    }

    #[test]
    fn unvoiced_packs_to_the_sentinel() {
        assert_eq!(pack(None), UNVOICED);
        assert_eq!(unpack(UNVOICED), None);
    }

    #[test]
    fn formats_names_on_octave_boundaries_and_sharps() {
        assert_eq!(format_name(59), "B3");
        assert_eq!(format_name(60), "C4");
        assert_eq!(format_name(61), "C#4");
        assert_eq!(format_name(71), "B4");
        assert_eq!(format_name(72), "C5");
        assert_eq!(format_name(0), "C-1");
        assert_eq!(format_name(-1), "B-2");
    }

    #[test]
    fn midi_from_hz_is_exact_on_equal_tempered_notes() {
        assert_eq!(midi_from_hz(440.0), 69.0);
        assert_eq!(midi_from_hz(880.0), 81.0);
        for midi in 24..=107 {
            let got = midi_from_hz(freq(midi, 0.0));
            // Only the f32 rounding of the frequency: ≪ 0.01¢.
            assert!((got - midi as f32).abs() < 1e-4, "MIDI {midi}: {got}");
            let quarter = midi_from_hz(freq(midi, 50.0));
            assert!(
                (quarter - (midi as f32 + 0.5)).abs() < 1e-4,
                "MIDI {midi}: {quarter}"
            );
        }
    }

    #[test]
    fn pack_midi_round_trips_over_midi() {
        for midi in 0..=127 {
            let packed = pack_midi(Some(midi));
            assert_ne!(packed, UNVOICED);
            assert_eq!(unpack_midi(packed), Some(midi));
        }
        assert_eq!(pack_midi(None), UNVOICED);
        assert_eq!(unpack_midi(UNVOICED), None);
    }
}
