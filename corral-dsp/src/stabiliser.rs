//! Chooses the note the harmony interval is picked from, once per hop,
//! with hysteresis so vibrato and scoops don't make it flicker.

/// A change needs the pitch this far from the stable note's centre…
const CHANGE_CENTS: f32 = 70.0;
/// …for at least this long…
const HOLD_S: f32 = 0.060;
/// …or this far for this many hops (the fast path)…
const FAST_CENTS: f32 = 150.0;
const FAST_HOPS: u32 = 3;
/// …and only once the pitch is within this of its nearest note, so a
/// scoop can't commit to a note it is passing through (see the plan).
const SETTLE_CENTS: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update {
    Held,
    Onset,
    Changed,
}

pub struct NoteStabiliser {
    note: Option<i32>,
    /// Consecutive hops beyond CHANGE_CENTS, and beyond FAST_CENTS.
    held: u32,
    fast: u32,
    /// ⌈HOLD_S · sr / hop⌉: 11 at 44.1 kHz (63.9 ms), 12 at 48 kHz (64 ms).
    hold_hops: u32,
}

impl NoteStabiliser {
    pub fn new(sample_rate: f32, hop_len: usize) -> Self {
        Self {
            note: None,
            held: 0,
            fast: 0,
            hold_hops: (HOLD_S * sample_rate / hop_len as f32).ceil() as u32,
        }
    }

    pub fn reset(&mut self) {
        self.note = None;
        self.clear_counters();
    }

    fn clear_counters(&mut self) {
        self.held = 0;
        self.fast = 0;
    }

    /// The stable MIDI note, if one has been chosen since reset.
    pub fn note(&self) -> Option<i32> {
        self.note
    }

    /// Feeds one voiced hop's pitch (fractional MIDI). `onset` is true
    /// when no harmony is sounding (the wet gate is closed): the nearest
    /// note is then taken at once.
    pub fn update(&mut self, pitch: f32, onset: bool) -> Update {
        let nearest = pitch.round() as i32;
        let note = match self.note {
            Some(note) if !onset => note,
            _ => {
                self.note = Some(nearest);
                self.clear_counters();
                return Update::Onset;
            }
        };
        let deviation = 100.0 * (pitch - note as f32).abs();
        if deviation <= CHANGE_CENTS {
            self.clear_counters();
            return Update::Held;
        }
        self.held += 1;
        self.fast = if deviation > FAST_CENTS {
            self.fast + 1
        } else {
            0
        };
        let settled = 100.0 * (pitch - nearest as f32).abs() <= SETTLE_CENTS;
        if settled && (self.held >= self.hold_hops || self.fast >= FAST_HOPS) {
            self.note = Some(nearest);
            self.clear_counters();
            return Update::Changed;
        }
        Update::Held
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const HOP: usize = 256;
    const SAMPLE_RATES: [f32; 2] = [44_100.0, 48_000.0];

    /// A stabiliser at 48 kHz that has already taken `note` at onset.
    fn on(note: f32) -> NoteStabiliser {
        let mut stabiliser = NoteStabiliser::new(48_000.0, HOP);
        assert_eq!(stabiliser.update(note, true), Update::Onset);
        stabiliser
    }

    /// Feeds `pitch` until the note changes; returns the hop it changed on
    /// (1-based), or `None` within `limit` hops.
    fn hops_to_change(stabiliser: &mut NoteStabiliser, pitch: f32, limit: u32) -> Option<u32> {
        (1..=limit).find(|_| stabiliser.update(pitch, false) == Update::Changed)
    }

    #[test]
    fn the_hold_is_11_hops_at_44k1_and_12_at_48k() {
        let hops = SAMPLE_RATES.map(|sr| NoteStabiliser::new(sr, HOP).hold_hops);
        println!("hold hops at 44.1 / 48 kHz: {hops:?}");
        assert_eq!(hops, [11, 12]);
    }

    #[test]
    fn onset_takes_the_nearest_note_at_once() {
        let mut stabiliser = NoteStabiliser::new(48_000.0, HOP);
        assert_eq!(stabiliser.note(), None);
        assert_eq!(stabiliser.update(60.4, false), Update::Onset);
        assert_eq!(stabiliser.note(), Some(60));
        assert_eq!(on(60.6).note(), Some(61));

        // A later onset re-picks at once, however close to the old note.
        assert_eq!(stabiliser.update(64.2, true), Update::Onset);
        assert_eq!(stabiliser.note(), Some(64));
        assert_eq!(stabiliser.update(63.9, true), Update::Onset);
        assert_eq!(stabiliser.note(), Some(64));
    }

    #[test]
    fn a_semitone_step_changes_on_the_last_hop_of_the_hold() {
        for sample_rate in SAMPLE_RATES {
            let mut stabiliser = NoteStabiliser::new(sample_rate, HOP);
            stabiliser.update(60.0, true);
            let hold = stabiliser.hold_hops;
            for hop in 1..hold {
                assert_eq!(stabiliser.update(61.0, false), Update::Held, "hop {hop}");
                assert_eq!(stabiliser.note(), Some(60));
            }
            assert_eq!(stabiliser.update(61.0, false), Update::Changed);
            assert_eq!(stabiliser.note(), Some(61));
        }
    }

    #[test]
    fn a_whole_tone_step_takes_the_fast_path_on_the_third_hop() {
        let mut stabiliser = on(60.0);
        assert_eq!(hops_to_change(&mut stabiliser, 62.0, 20), Some(FAST_HOPS));
        assert_eq!(stabiliser.note(), Some(62));
        // And downward.
        assert_eq!(hops_to_change(&mut stabiliser, 60.0, 20), Some(FAST_HOPS));
        assert_eq!(stabiliser.note(), Some(60));
    }

    /// Per-hop pitches of a sinusoidal vibrato around `centre`.
    fn vibrato(sample_rate: f32, centre: f32, cents: f32, hz: f32, seconds: f32) -> Vec<f32> {
        let hop_s = HOP as f32 / sample_rate;
        (0..(seconds / hop_s) as usize)
            .map(|k| centre + cents / 100.0 * (TAU * hz * k as f32 * hop_s).sin())
            .collect()
    }

    #[test]
    fn a_semitone_of_vibrato_never_changes_the_note() {
        for sample_rate in SAMPLE_RATES {
            for hz in [4.5, 5.0, 6.0, 7.0] {
                let pitches = vibrato(sample_rate, 60.0, 100.0, hz, 3.0);
                let mut stabiliser = NoteStabiliser::new(sample_rate, HOP);
                stabiliser.update(pitches[0], true);
                let mut longest = 0;
                for &pitch in &pitches[1..] {
                    stabiliser.update(pitch, false);
                    longest = longest.max(stabiliser.held);
                    assert_eq!(stabiliser.note(), Some(60), "{hz} Hz at {sample_rate}");
                }
                println!(
                    "vibrato ±100¢ {hz} Hz at {sample_rate}: longest run {longest} of {} hops",
                    stabiliser.hold_hops
                );
            }
        }
    }

    #[test]
    fn vibrato_within_the_threshold_never_starts_the_counter() {
        let pitches = vibrato(48_000.0, 60.0, 69.0, 5.0, 3.0);
        let mut stabiliser = on(pitches[0]);
        for &pitch in &pitches[1..] {
            assert_eq!(stabiliser.update(pitch, false), Update::Held);
            assert_eq!(stabiliser.held, 0, "{pitch}");
        }
    }

    #[test]
    fn brief_excursions_reset_the_counter() {
        for length in 1..=10 {
            let mut stabiliser = on(60.0);
            for _ in 0..3 {
                for _ in 0..length {
                    assert_eq!(stabiliser.update(61.0, false), Update::Held);
                }
                stabiliser.update(60.0, false);
                assert_eq!(stabiliser.held, 0);
            }
            assert_eq!(stabiliser.note(), Some(60), "{length} hops");
        }
    }

    #[test]
    fn a_pitch_between_notes_never_commits_until_it_settles() {
        let mut stabiliser = on(60.0);
        assert_eq!(hops_to_change(&mut stabiliser, 61.5, 200), None);
        assert_eq!(stabiliser.note(), Some(60));
        // Within 30¢ of 62, with the hold long since satisfied: at once.
        assert_eq!(hops_to_change(&mut stabiliser, 61.8, 1), Some(1));
        assert_eq!(stabiliser.note(), Some(62));
    }

    #[test]
    fn a_linear_scoop_changes_at_most_once_and_ends_on_the_target() {
        let target = 64.0;
        for sample_rate in SAMPLE_RATES {
            for offset in 0..HOP {
                let mut stabiliser = NoteStabiliser::new(sample_rate, HOP);
                let mut changes = Vec::new();
                // −200¢ → 0 over 150 ms, then held for 0.5 s.
                for k in 0..((0.65 * sample_rate) as usize / HOP) {
                    let t = (k * HOP + offset) as f32 / sample_rate;
                    let pitch = target - 2.0 + 2.0 * (t / 0.150).min(1.0);
                    if stabiliser.update(pitch, k == 0) == Update::Changed {
                        changes.push((k, stabiliser.note()));
                    }
                }
                assert!(
                    changes.len() <= 1,
                    "{sample_rate} offset {offset}: {changes:?}"
                );
                assert_eq!(stabiliser.note(), Some(target as i32));
            }
        }
    }

    #[test]
    fn reset_forgets_the_note() {
        let mut stabiliser = on(60.0);
        stabiliser.update(61.0, false);
        stabiliser.reset();
        assert_eq!(stabiliser.note(), None);
        assert_eq!(stabiliser.held, 0);
        assert_eq!(stabiliser.update(65.0, false), Update::Onset);
    }
}
