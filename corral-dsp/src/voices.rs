//! The harmony voices over one shared analysis: which intervals sound,
//! how they follow the stable note and grid edits, and their fades.
//!
//! The sounding voices follow one grid row, exactly one voice per "on"
//! interval. When the stable note changes, the old voices take the new
//! row's intervals by position (ascending to ascending) and glide there;
//! extra intervals get new voices that fade in, and unmatched old voices
//! fade out. An edit of the sounding row only fades the edited cell's
//! voice. At voicing onset matched voices jump instead of gliding.

use std::f32::consts::FRAC_PI_2;

use crate::grid::{MAX_VOICES, Row};
use crate::psola::{self, Analysis};
use crate::shift::{self, Glide};

/// Voices fade in and out over this long, equal-power (as the wet gate).
const FADE_S: f32 = 0.020;
/// Room for a full row sounding while as many voices fade out. Fading
/// voices can outnumber a row only when note changes come faster than a
/// fade (see the `slots` test); beyond this the quietest one is stolen.
const SLOTS: usize = 2 * MAX_VOICES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Free,
    Sounding,
    FadingOut,
}

struct Slot {
    phase: Phase,
    /// Where it glides to: its interval in the row while sounding.
    interval: i8,
    glide: Glide,
    /// 0.0 = silent … 1.0 = full; moves by `step` each sample.
    position: f32,
    synth: psola::Voice,
}

impl Slot {
    fn new() -> Self {
        Self {
            phase: Phase::Free,
            interval: 0,
            glide: Glide::new(0.0),
            position: 0.0,
            synth: psola::Voice::new(),
        }
    }

    fn is_live(&self) -> bool {
        self.phase != Phase::Free
    }

    fn free(&mut self) {
        self.phase = Phase::Free;
        self.position = 0.0;
        self.synth.stop();
    }

    /// sin(position · π/2), exactly 0.0 and 1.0 at rest, as
    /// `WetGate::next_gains`.
    fn gain(&self) -> f32 {
        let p = self.position;
        if p == 0.0 {
            0.0
        } else if p == 1.0 {
            1.0
        } else {
            (p * FRAC_PI_2).sin()
        }
    }
}

/// One live voice, for the tests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceState {
    pub interval: i8,
    /// The glide's value.
    pub semitones: f32,
    /// sin(position · π/2); exactly 0.0 and 1.0 at rest.
    pub gain: f32,
    /// False while fading out.
    pub sounding: bool,
}

pub struct VoicePool {
    /// Boxed, allocated in `new` (from `Chain::new`, in `activate()`).
    slots: Box<[Slot]>,
    /// The row the sounding voices follow: exactly one sounding slot per
    /// interval in it.
    row: Row,
    step: f32,
    /// Fading voices stolen for want of a free slot.
    #[cfg(test)]
    steals: usize,
}

impl VoicePool {
    /// Allocates.
    pub fn new(sample_rate: f32) -> Self {
        Self {
            slots: (0..SLOTS).map(|_| Slot::new()).collect(),
            row: Row::EMPTY,
            step: 1.0 / (FADE_S * sample_rate),
            #[cfg(test)]
            steals: 0,
        }
    }

    /// Frees every voice.
    pub fn reset(&mut self) {
        for slot in self.slots.iter_mut() {
            slot.free();
        }
        self.row = Row::EMPTY;
    }

    pub fn row(&self) -> Row {
        self.row
    }

    /// The sounding slot at `interval`. Exactly one exists for each
    /// interval in `self.row`.
    fn sounding_slot(&self, interval: i8) -> usize {
        self.slots
            .iter()
            .position(|s| s.phase == Phase::Sounding && s.interval == interval)
            .expect("one sounding slot per interval in the row")
    }

    /// Picks a slot for a new voice at `interval`: a voice fading out at
    /// that interval (revived, keeping its level and synthesis), else a
    /// free slot, else the quietest fading voice (stolen).
    fn claim(&mut self, interval: i8) -> usize {
        let fading = |s: &Slot| s.phase == Phase::FadingOut;
        if let Some(index) = self
            .slots
            .iter()
            .position(|s| fading(s) && s.interval == interval)
        {
            self.slots[index].phase = Phase::Sounding;
            return index;
        }
        let index = match self.slots.iter().position(|s| s.phase == Phase::Free) {
            Some(index) => index,
            None => {
                #[cfg(test)]
                {
                    self.steals += 1;
                }
                self.slots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| fading(s))
                    .min_by(|a, b| a.1.position.total_cmp(&b.1.position))
                    .map(|(index, _)| index)
                    .expect("at most MAX_VOICES of SLOTS are sounding")
            }
        };
        let slot = &mut self.slots[index];
        slot.phase = Phase::Sounding;
        slot.interval = interval;
        slot.glide.jump(f32::from(interval));
        slot.position = 0.0;
        slot.synth.stop();
        index
    }

    /// Matches the sounding voices to `row` by position. Matched voices
    /// take their new interval (and jump there if `jump`), extra
    /// intervals are claimed, and unmatched voices fade out.
    fn rematch(&mut self, row: Row, jump: bool) {
        let mut old = [0; MAX_VOICES];
        let count = self.row.len();
        for (index, interval) in old.iter_mut().zip(self.row.intervals()) {
            *index = self.sounding_slot(interval);
        }
        let mut new = row.intervals();
        for &index in &old[..count] {
            let slot = &mut self.slots[index];
            match new.next() {
                Some(interval) => {
                    slot.interval = interval;
                    if jump {
                        slot.glide.jump(f32::from(interval));
                    }
                }
                None => slot.phase = Phase::FadingOut,
            }
        }
        for interval in new {
            self.claim(interval);
        }
        self.row = row;
    }

    /// Voicing onset: as `change`, but matched voices jump to their new
    /// intervals. When `silent` (no wet signal audible), fading voices
    /// are freed and every sounding voice is at full level at once.
    pub fn onset(&mut self, row: Row, silent: bool) {
        if silent {
            for slot in self.slots.iter_mut() {
                if slot.phase == Phase::FadingOut {
                    slot.free();
                }
            }
        }
        self.rematch(row, true);
        for slot in self.slots.iter_mut() {
            match slot.phase {
                Phase::FadingOut if silent => slot.free(),
                Phase::Sounding => {
                    // A revived voice may still be gliding.
                    slot.glide.jump(f32::from(slot.interval));
                    if silent {
                        slot.position = 1.0;
                    }
                }
                _ => {}
            }
        }
    }

    /// The stable note changed. The sounding voices (ascending) take
    /// `row`'s intervals (ascending) by position and glide there. Extra
    /// intervals get new voices, which jump there and fade in. Unmatched
    /// old voices fade out.
    pub fn change(&mut self, row: Row) {
        self.rematch(row, false);
    }

    /// The sounding row was edited. Intervals no longer on fade out;
    /// newly on intervals get voices that fade in. Every other voice is
    /// untouched.
    pub fn edit(&mut self, row: Row) {
        let (old, new) = (self.row.bits(), row.bits());
        for interval in Row::from_bits(old & !new).intervals() {
            let index = self.sounding_slot(interval);
            self.slots[index].phase = Phase::FadingOut;
        }
        for interval in Row::from_bits(new & !old).intervals() {
            self.claim(interval);
        }
        self.row = row;
    }

    /// Every live voice starts synthesising at `mark` (the analysis has
    /// just started).
    pub fn start_all(&mut self, mark: i64) {
        for slot in self.slots.iter_mut().filter(|s| s.is_live()) {
            slot.synth.start(mark);
        }
    }

    pub fn stop_all(&mut self) {
        for slot in self.slots.iter_mut() {
            slot.synth.stop();
        }
    }

    /// Advances every live voice's glide and fade by one sample. With
    /// `synthesis` (the analysis and the period), it also returns
    /// Σ gain · voice; a live voice that isn't synthesising yet starts at
    /// `analysis.entry_mark()`. Frees voices whose fade-out has finished.
    #[inline]
    pub fn next(&mut self, synthesis: Option<(&Analysis, f32)>, coefficient: f32) -> f32 {
        // From −0.0, so one voice at gain 1.0 passes through bit for bit,
        // the sign of zero included.
        let mut sum = -0.0;
        let step = self.step;
        for slot in self.slots.iter_mut().filter(|s| s.is_live()) {
            slot.position = match slot.phase {
                Phase::Sounding => (slot.position + step).min(1.0),
                _ => (slot.position - step).max(0.0),
            };
            let semitones = slot.glide.next(f32::from(slot.interval), coefficient);
            if slot.phase == Phase::FadingOut && slot.position == 0.0 {
                slot.free();
                continue;
            }
            if let Some((analysis, period)) = synthesis {
                if !slot.synth.is_running() {
                    slot.synth.start(analysis.entry_mark());
                }
                let shifted = slot.synth.next(analysis, period, shift::ratio(semitones));
                sum += slot.gain() * shifted;
            }
        }
        sum
    }

    pub fn states(&self) -> impl Iterator<Item = VoiceState> + '_ {
        self.slots
            .iter()
            .filter(|s| s.is_live())
            .map(|s| VoiceState {
                interval: s.interval,
                semitones: s.glide.current(),
                gain: s.gain(),
                sounding: s.phase == Phase::Sounding,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::INTERVALS;
    use crate::shift::{DEFAULT_GLIDE_S, glide_coefficient};

    const SR: f32 = 48_000.0;

    fn row(intervals: &[i8]) -> Row {
        intervals
            .iter()
            .fold(Row::EMPTY, |row, &i| row.with(i, true))
    }

    fn coefficient() -> f32 {
        glide_coefficient(DEFAULT_GLIDE_S, SR)
    }

    /// The live slot indices, in slot order.
    fn live(pool: &VoicePool) -> Vec<usize> {
        (0..SLOTS).filter(|&i| pool.slots[i].is_live()).collect()
    }

    /// The live slot at `interval` that is (or isn't) sounding.
    fn slot_at(pool: &VoicePool, interval: i8, sounding: bool) -> usize {
        let phase = if sounding {
            Phase::Sounding
        } else {
            Phase::FadingOut
        };
        (0..SLOTS)
            .find(|&i| pool.slots[i].phase == phase && pool.slots[i].interval == interval)
            .unwrap_or_else(|| panic!("no slot at {interval:+} (sounding {sounding})"))
    }

    fn state(pool: &VoicePool, index: usize) -> VoiceState {
        let slot = &pool.slots[index];
        VoiceState {
            interval: slot.interval,
            semitones: slot.glide.current(),
            gain: slot.gain(),
            sounding: slot.phase == Phase::Sounding,
        }
    }

    fn samples(seconds: f32, sample_rate: f32) -> usize {
        (seconds * sample_rate).round() as usize
    }

    /// Runs `next` until slot `index`'s fade settles (or it is freed);
    /// returns its gains along the way.
    fn run_fade(pool: &mut VoicePool, index: usize) -> Vec<f32> {
        let mut gains = Vec::new();
        for _ in 0..100_000 {
            pool.next(None, coefficient());
            let slot = &pool.slots[index];
            gains.push(slot.gain());
            if !slot.is_live() || slot.position == 1.0 {
                return gains;
            }
        }
        panic!("the fade never settled");
    }

    #[test]
    fn onset_while_silent_starts_at_full_level_on_the_intervals() {
        let mut pool = VoicePool::new(SR);
        pool.onset(row(&[-5, 4]), true);
        let c = coefficient();
        for _ in 0..1000 {
            pool.next(None, c);
            let mut states: Vec<VoiceState> = pool.states().collect();
            states.sort_by_key(|s| s.interval);
            assert_eq!(states.len(), 2);
            for (state, interval) in states.iter().zip([-5, 4]) {
                assert_eq!(state.interval, interval);
                assert_eq!(state.semitones, f32::from(interval));
                assert_eq!(state.gain, 1.0);
                assert!(state.sounding);
            }
        }
    }

    #[test]
    fn onset_while_fading_jumps_matched_voices_and_fades_surplus() {
        let mut pool = VoicePool::new(SR);
        pool.onset(row(&[-5, 4, 7]), true);
        pool.next(None, coefficient());
        let (low, mid, high) = (
            slot_at(&pool, -5, true),
            slot_at(&pool, 4, true),
            slot_at(&pool, 7, true),
        );
        pool.onset(row(&[3, 12]), false);
        pool.next(None, coefficient());
        // By position: −5 → +3, +4 → +12, +7 surplus.
        assert_eq!(state(&pool, low).semitones, 3.0);
        assert_eq!(state(&pool, low).gain, 1.0);
        assert_eq!(state(&pool, mid).semitones, 12.0);
        let surplus = state(&pool, high);
        assert!(!surplus.sounding);
        assert!(0.0 < surplus.gain && surplus.gain < 1.0, "{surplus:?}");
        let gains = run_fade(&mut pool, high);
        assert!(!pool.slots[high].is_live());
        assert!(gains.windows(2).all(|w| w[1] <= w[0]));
    }

    #[test]
    fn change_of_the_same_size_glides_the_same_slot() {
        let mut pool = VoicePool::new(SR);
        pool.onset(row(&[4]), true);
        let slot = slot_at(&pool, 4, true);
        pool.change(row(&[3]));
        assert_eq!(slot_at(&pool, 3, true), slot);
        pool.next(None, coefficient());
        let expected = 4.0 - (1.0 - coefficient());
        let got = state(&pool, slot).semitones;
        assert!((got - expected).abs() <= 1e-6, "{got} vs {expected}");
        assert_eq!(live(&pool), [slot]);
    }

    #[test]
    fn change_growing_fades_a_new_voice_in_over_the_fade_time() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            let mut pool = VoicePool::new(sample_rate);
            pool.onset(row(&[4]), true);
            let old = slot_at(&pool, 4, true);
            pool.change(row(&[3, 7]));
            assert_eq!(pool.slots[old].interval, 3);
            let new = slot_at(&pool, 7, true);
            assert_ne!(new, old);
            assert_eq!(state(&pool, new).semitones, 7.0);
            assert_eq!(state(&pool, new).gain, 0.0);
            let gains = run_fade(&mut pool, new);
            assert!(gains.windows(2).all(|w| w[1] >= w[0]), "{sample_rate}");
            assert_eq!(*gains.last().unwrap(), 1.0);
            let expected = samples(FADE_S, sample_rate);
            assert!(
                gains.len().abs_diff(expected) <= 1,
                "{sample_rate}: {} samples vs {expected}",
                gains.len()
            );
            assert_eq!(state(&pool, new).semitones, 7.0);
        }
    }

    #[test]
    fn change_shrinking_fades_the_surplus_voice_out_and_frees_it() {
        let mut pool = VoicePool::new(SR);
        pool.onset(row(&[3, 7]), true);
        let (low, high) = (slot_at(&pool, 3, true), slot_at(&pool, 7, true));
        pool.change(row(&[4]));
        assert_eq!(slot_at(&pool, 4, true), low);
        assert_eq!(slot_at(&pool, 7, false), high);
        let gains = run_fade(&mut pool, high);
        assert!(gains.windows(2).all(|w| w[1] <= w[0]));
        assert!(gains.len().abs_diff(samples(FADE_S, SR)) <= 1);
        assert_eq!(live(&pool), [low]);
    }

    #[test]
    fn change_matches_by_position_not_value() {
        let mut pool = VoicePool::new(SR);
        pool.onset(row(&[-5, 4]), true);
        let (low, high) = (slot_at(&pool, -5, true), slot_at(&pool, 4, true));
        pool.change(row(&[-12, -5, 4, 12]));
        assert_eq!(pool.slots[low].interval, -12);
        assert_eq!(pool.slots[high].interval, -5);
        for interval in [4, 12] {
            let new = slot_at(&pool, interval, true);
            assert!(new != low && new != high);
            assert_eq!(state(&pool, new).gain, 0.0);
            assert_eq!(state(&pool, new).semitones, f32::from(interval));
        }
        assert_eq!(live(&pool).len(), 4);
    }

    #[test]
    fn an_octave_leap_moves_nothing() {
        let mut pool = VoicePool::new(SR);
        pool.onset(row(&[-5, 4]), true);
        let before = live(&pool);
        pool.change(row(&[-5, 4]));
        pool.next(None, coefficient());
        assert_eq!(live(&pool), before);
        for index in before {
            let s = state(&pool, index);
            assert_eq!((s.semitones, s.gain), (f32::from(s.interval), 1.0));
        }
    }

    #[test]
    fn edit_works_by_value() {
        let mut pool = VoicePool::new(SR);
        pool.onset(row(&[-5, 4]), true);
        let (low, high) = (slot_at(&pool, -5, true), slot_at(&pool, 4, true));
        pool.edit(row(&[-5, 4, 7]));
        let added = slot_at(&pool, 7, true);
        let gains = run_fade(&mut pool, added);
        assert!(gains.windows(2).all(|w| w[1] >= w[0]));
        assert!(gains.len().abs_diff(samples(FADE_S, SR)) <= 1);
        for index in [low, high] {
            assert_eq!(state(&pool, index).gain, 1.0);
            assert!(state(&pool, index).sounding);
        }
        assert_eq!(pool.slots[low].interval, -5);
        assert_eq!(pool.slots[high].interval, 4);

        pool.edit(row(&[4, 7]));
        assert_eq!(slot_at(&pool, -5, false), low);
        let gains = run_fade(&mut pool, low);
        assert!(gains.windows(2).all(|w| w[1] <= w[0]));
        assert!(!pool.slots[low].is_live());
        assert_eq!(state(&pool, high).gain, 1.0);
        assert_eq!(state(&pool, added).gain, 1.0);
    }

    #[test]
    fn switching_a_cell_back_on_revives_its_fading_voice() {
        let mut pool = VoicePool::new(SR);
        pool.onset(row(&[4, 7]), true);
        let seven = slot_at(&pool, 7, true);
        pool.edit(row(&[4]));
        let mut lowest = 1.0f32;
        for _ in 0..samples(0.005, SR) {
            pool.next(None, coefficient());
            lowest = lowest.min(state(&pool, seven).gain);
        }
        let before = live(&pool);
        pool.edit(row(&[4, 7]));
        assert_eq!(slot_at(&pool, 7, true), seven);
        assert_eq!(live(&pool), before, "no new slot");
        let gains = run_fade(&mut pool, seven);
        assert!(gains[0] > lowest, "rises from where it was");
        assert!(gains.iter().all(|&g| g > 0.0));
        assert!(gains.windows(2).all(|w| w[1] >= w[0]));
        assert_eq!(*gains.last().unwrap(), 1.0);
    }

    /// Voices fade out faster than they're replaced only when changes
    /// match sounding voices onto intervals whose voices are still fading.
    /// Full → the upper half → nothing → full, all in one sample:
    /// each round leaves 12 more voices fading at +1…+12, so the second
    /// round fills all 48 slots and the third has to steal.
    #[test]
    fn slots_cover_rapid_changes_then_steal() {
        let full = Row::from_bits(u32::MAX);
        let upper = row(&INTERVALS[12..]);
        let mut pool = VoicePool::new(SR);
        pool.onset(full, true);
        assert_eq!(live(&pool).len(), 24);
        for (round, expected) in [(1, 36), (2, 48)] {
            for next in [upper, Row::EMPTY, full] {
                pool.change(next);
            }
            assert_eq!(live(&pool).len(), expected, "round {round}");
            assert_eq!(pool.steals, 0, "round {round}");
        }
        for next in [upper, Row::EMPTY, full] {
            pool.change(next);
        }
        assert_eq!(pool.steals, 12);
        assert_eq!(live(&pool).len(), SLOTS);
        assert_eq!(pool.states().filter(|s| s.sounding).count(), 24);
    }

    #[test]
    fn reset_frees_everything() {
        let mut pool = VoicePool::new(SR);
        pool.onset(row(&[-5, 4, 7]), true);
        pool.edit(row(&[-5]));
        pool.reset();
        assert!(live(&pool).is_empty());
        assert_eq!(pool.row(), Row::EMPTY);
        assert_eq!(pool.states().count(), 0);
        assert_eq!(
            pool.next(None, coefficient()).to_bits(),
            (-0.0f32).to_bits()
        );
    }
}
