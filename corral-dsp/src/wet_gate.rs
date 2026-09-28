//! Chooses between the delayed dry signal and the shifted signal, with
//! hysteresis on the voicing decision and an equal-power crossfade.

use std::f32::consts::FRAC_PI_2;

const CROSSFADE_S: f32 = 0.010;
/// Consecutive voiced hops needed to open, and unvoiced hops to close.
const OPEN_HOPS: u32 = 2;
const CLOSE_HOPS: u32 = 2;

pub struct WetGate {
    open: bool,
    /// Consecutive hops that disagreed with `open`.
    run: u32,
    /// 0.0 = all dry, 1.0 = all shifted.
    position: f32,
    /// Fade per sample.
    step: f32,
}

impl WetGate {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            open: false,
            run: 0,
            position: 0.0,
            step: 1.0 / (CROSSFADE_S * sample_rate),
        }
    }

    pub fn reset(&mut self) {
        self.open = false;
        self.run = 0;
        self.position = 0.0;
    }

    /// Feeds one hop's voicing decision.
    pub fn update(&mut self, voiced: bool) {
        if voiced == self.open {
            self.run = 0;
            return;
        }
        self.run += 1;
        let needed = if voiced { OPEN_HOPS } else { CLOSE_HOPS };
        if self.run >= needed {
            self.open = voiced;
            self.run = 0;
        }
    }

    /// Whether the gate wants the shifted signal (after hysteresis).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// True while any shifted signal is audible (open or still fading out).
    pub fn is_active(&self) -> bool {
        self.open || self.position > 0.0
    }

    /// Advances the fade by one sample. Returns (dry_gain, shifted_gain),
    /// with cos/sin of position · π/2; exactly (1.0, 0.0) at rest closed and
    /// (0.0, 1.0) at rest open.
    #[inline]
    pub fn next_gains(&mut self) -> (f32, f32) {
        self.position = if self.open {
            (self.position + self.step).min(1.0)
        } else {
            (self.position - self.step).max(0.0)
        };
        if self.position == 0.0 {
            (1.0, 0.0)
        } else if self.position == 1.0 {
            (0.0, 1.0)
        } else {
            let (sin, cos) = (self.position * FRAC_PI_2).sin_cos();
            (cos, sin)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Runs `next_gains` until the fade settles; returns the samples taken and
    /// every gain pair along the way.
    fn run_fade(gate: &mut WetGate) -> (usize, Vec<(f32, f32)>) {
        let target = if gate.is_open() { 1.0 } else { 0.0 };
        let mut gains = Vec::new();
        for i in 1..100_000 {
            gains.push(gate.next_gains());
            if gate.position == target {
                return (i, gains);
            }
        }
        panic!("the fade never settled");
    }

    #[test]
    fn one_voiced_hop_doesnt_open_it_two_do() {
        let mut gate = WetGate::new(SR);
        gate.update(true);
        assert!(!gate.is_open());
        gate.update(true);
        assert!(gate.is_open());
    }

    #[test]
    fn one_unvoiced_hop_doesnt_close_it_two_do() {
        let mut gate = WetGate::new(SR);
        gate.update(true);
        gate.update(true);
        gate.update(false);
        assert!(gate.is_open());
        gate.update(false);
        assert!(!gate.is_open());
    }

    #[test]
    fn alternating_hops_never_open_it() {
        let mut gate = WetGate::new(SR);
        for i in 0..100 {
            gate.update(i % 2 == 0);
            assert!(!gate.is_open(), "hop {i}");
        }
    }

    #[test]
    fn alternating_hops_never_close_it() {
        let mut gate = WetGate::new(SR);
        gate.update(true);
        gate.update(true);
        for i in 0..100 {
            gate.update(i % 2 == 1);
            assert!(gate.is_open(), "hop {i}");
        }
    }

    #[test]
    fn the_fade_takes_the_crossfade_time_both_ways() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            let expected = CROSSFADE_S * sample_rate;
            let mut gate = WetGate::new(sample_rate);
            gate.update(true);
            gate.update(true);
            let (samples, _) = run_fade(&mut gate);
            assert!((samples as f32 - expected).abs() <= 1.0, "open: {samples}");
            gate.update(false);
            gate.update(false);
            let (samples, _) = run_fade(&mut gate);
            assert!((samples as f32 - expected).abs() <= 1.0, "close: {samples}");
        }
    }

    #[test]
    fn the_fade_is_equal_power_throughout() {
        let mut gate = WetGate::new(SR);
        gate.update(true);
        gate.update(true);
        let (_, opening) = run_fade(&mut gate);
        assert_eq!(*opening.last().unwrap(), (0.0, 1.0));
        gate.update(false);
        gate.update(false);
        let (_, closing) = run_fade(&mut gate);
        for (dry, shifted) in opening.into_iter().chain(closing) {
            let power = dry * dry + shifted * shifted;
            assert!((power - 1.0).abs() <= 1e-6, "{dry}, {shifted}");
        }
    }

    #[test]
    fn at_rest_closed_it_is_exactly_dry_and_inactive() {
        let mut gate = WetGate::new(SR);
        for _ in 0..10 {
            assert_eq!(gate.next_gains(), (1.0, 0.0));
        }
        assert!(!gate.is_active());

        // After a full open and close, it is exactly dry again.
        gate.update(true);
        gate.update(true);
        assert!(gate.is_active());
        run_fade(&mut gate);
        gate.update(false);
        gate.update(false);
        assert!(gate.is_active(), "still fading out");
        run_fade(&mut gate);
        assert!(!gate.is_active());
        let (dry, shifted) = gate.next_gains();
        assert_eq!((dry.to_bits(), shifted.to_bits()), (1.0f32.to_bits(), 0));
    }

    #[test]
    fn reset_closes_it_at_once() {
        let mut gate = WetGate::new(SR);
        gate.update(true);
        gate.update(true);
        run_fade(&mut gate);
        gate.reset();
        assert!(!gate.is_open());
        assert!(!gate.is_active());
        assert_eq!(gate.next_gains(), (1.0, 0.0));
        // A lone voiced hop after reset doesn't open it: the run was cleared.
        gate.update(true);
        assert!(!gate.is_open());
    }
}
