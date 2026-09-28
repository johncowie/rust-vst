//! TD-PSOLA pitch shifting. See docs/td-psola.md.
//!
//! All time indices are `i64` input-sample indices since reset. The newest
//! input is `n = written − 1`, and the output being produced is `o = n − L`.

use std::f64::consts::PI;

use crate::pitch::{MAX_F0_HZ, MIN_F0_HZ};

/// Latency in periods of the lowest detectable pitch. A 2P grain centred on
/// a mark needs P samples after the mark, and the mark needs up to 1.25P of
/// lookahead to be confirmed; see the plan for 0005.
const LATENCY_PERIODS: f32 = 2.0;
/// ± this fraction of a period around previous mark + P is searched for the next mark.
const MARK_SEARCH: f32 = 0.25;
/// Grains alive at once: 2P / (P / r_max) + 1 = 5 at r = 2; 8 leaves headroom.
const MAX_GRAINS: usize = 8;
/// Confirmed marks kept (from o − 2·P_max to n); sized for P_min at 1 kHz.
const MAX_MARKS: usize = 128;
/// The detector voices estimates up to this factor above `MAX_F0_HZ`.
const RANGE_MARGIN: f32 = 1.03;

/// The chain's fixed latency. Depends only on the sample rate.
pub fn latency_samples(sample_rate: f32) -> usize {
    (LATENCY_PERIODS * sample_rate / MIN_F0_HZ).ceil() as usize
}

/// The longest period the detector reports, rounded up, in samples.
fn max_period(sample_rate: f32) -> i64 {
    (sample_rate / MIN_F0_HZ).ceil() as i64
}

#[derive(Clone, Copy, Default)]
struct Grain {
    /// Output time of the synthesis mark.
    centre: f64,
    /// Input time of the analysis mark it copies.
    source: i64,
    half: f32,
    live: bool,
}

pub struct Psola {
    latency: i64,
    max_period: i64,
    /// Mono input, power-of-two ring ≥ L + 2·P_max + 4.
    input: Vec<f32>,
    mask: usize,
    written: i64,
    /// Confirmed analysis marks, oldest first (fixed-size ring).
    marks: [i64; MAX_MARKS],
    mark_head: usize,
    mark_len: usize,
    /// +1.0 or −1.0: marks sit on peaks of this sign.
    polarity: f32,
    grains: [Grain; MAX_GRAINS],
    /// Output time of the next synthesis mark.
    next_synthesis: f64,
    running: bool,
}

impl Psola {
    /// Allocates. Call from `Chain::new`.
    pub fn new(sample_rate: f32) -> Self {
        let latency = latency_samples(sample_rate) as i64;
        let max_period = max_period(sample_rate);
        let ring_len = (latency + 2 * max_period + 4) as usize;
        let ring_len = ring_len.next_power_of_two();
        debug_assert!(
            (latency + 2 * max_period) as f32
                / (sample_rate / (MAX_F0_HZ * RANGE_MARGIN))
                / (1.0 - MARK_SEARCH)
                < MAX_MARKS as f32,
            "the mark ring is too small at {sample_rate} Hz"
        );
        Self {
            latency,
            max_period,
            input: vec![0.0; ring_len],
            mask: ring_len - 1,
            written: 0,
            marks: [0; MAX_MARKS],
            mark_head: 0,
            mark_len: 0,
            polarity: 1.0,
            grains: [Grain::default(); MAX_GRAINS],
            next_synthesis: 0.0,
            running: false,
        }
    }

    pub fn reset(&mut self) {
        self.input.fill(0.0);
        self.written = 0;
        self.stop();
    }

    /// The newest input sample's index.
    fn newest(&self) -> i64 {
        self.written - 1
    }

    /// The output time being produced.
    fn output_time(&self) -> i64 {
        self.newest() - self.latency
    }

    /// The input sample at `index`; 0.0 before the first sample.
    #[inline]
    fn at(&self, index: i64) -> f32 {
        if index < 0 {
            0.0
        } else {
            self.input[index as usize & self.mask]
        }
    }

    /// The input at a fractional index, linearly interpolated, clamped to ≤ n.
    #[inline]
    fn read(&self, position: f64) -> f32 {
        let position = position.min(self.newest() as f64);
        let floor = position.floor();
        let index = floor as i64;
        let frac = (position - floor) as f32;
        let x0 = self.at(index);
        if frac == 0.0 {
            x0
        } else {
            x0 + frac * (self.at(index + 1) - x0)
        }
    }

    fn last_mark(&self) -> Option<i64> {
        (self.mark_len > 0).then(|| self.marks[(self.mark_head + self.mark_len - 1) % MAX_MARKS])
    }

    fn mark(&self, i: usize) -> i64 {
        self.marks[(self.mark_head + i) % MAX_MARKS]
    }

    fn push_mark(&mut self, mark: i64) {
        if self.mark_len == MAX_MARKS {
            debug_assert!(false, "the mark ring overflowed");
            self.mark_head = (self.mark_head + 1) % MAX_MARKS;
            self.mark_len -= 1;
        }
        self.marks[(self.mark_head + self.mark_len) % MAX_MARKS] = mark;
        self.mark_len += 1;
    }

    /// The index in `[from, to]` where `sign · x` is largest (the first on ties).
    fn argmax(&self, from: i64, to: i64, value: impl Fn(f32) -> f32) -> i64 {
        let mut best = from;
        let mut best_value = value(self.at(from));
        for index in from + 1..=to {
            let v = value(self.at(index));
            if v > best_value {
                best = index;
                best_value = v;
            }
        }
        best
    }

    /// Confirms every mark whose whole search window has arrived.
    fn confirm_marks(&mut self, period: f32) {
        let step = period.round().max(1.0) as i64;
        let reach = (MARK_SEARCH * period).ceil() as i64;
        let polarity = self.polarity;
        while let Some(last) = self.last_mark() {
            let expected = last + step;
            if expected + reach > self.newest() {
                break;
            }
            // `reach` < `step`, so marks always move forward.
            let mark = self.argmax(expected - reach, expected + reach, |x| polarity * x);
            self.push_mark(mark);
        }
    }

    /// Stores one mono input sample (non-finite → 0.0). If running, confirms
    /// any marks now within reach: while last + P + ⌈MARK_SEARCH·P⌉ ≤ n,
    /// pushes argmax(polarity · x) over last + P ± MARK_SEARCH·P.
    pub fn push(&mut self, sample: f32, period: f32) {
        let index = self.written as usize & self.mask;
        self.input[index] = if sample.is_finite() { sample } else { 0.0 };
        self.written += 1;
        if self.running {
            self.confirm_marks(period);
        }
    }

    /// Starts shifting at the current output time. The first mark is
    /// argmax |x| over [o − ⌈P⌉, o]; its sign sets `polarity`. Then marks are
    /// confirmed up to n, a catch-up burst of at most ~(L + P) / P searches.
    pub fn start(&mut self, period: f32) {
        self.stop();
        let o = self.output_time();
        let first = self.argmax(o - period.ceil() as i64, o, f32::abs);
        self.polarity = if self.at(first) < 0.0 { -1.0 } else { 1.0 };
        self.push_mark(first);
        self.next_synthesis = first as f64;
        self.running = true;
        self.confirm_marks(period);
    }

    /// Stops and forgets marks and grains.
    pub fn stop(&mut self) {
        self.running = false;
        self.mark_head = 0;
        self.mark_len = 0;
        self.grains = [Grain::default(); MAX_GRAINS];
        self.next_synthesis = 0.0;
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// The index of the confirmed mark nearest to output time `centre`.
    fn nearest_mark(&self, centre: f64) -> usize {
        let mut best = 0;
        for i in 1..self.mark_len {
            let mark = self.mark(i);
            if (mark as f64 - centre).abs() < (self.mark(best) as f64 - centre).abs() {
                best = i;
            } else if mark as f64 > centre {
                break; // marks are sorted, so they only get further away
            }
        }
        best
    }

    /// The period the marks themselves show around mark `i`: the spacing to
    /// the next mark, or from the previous one, or `period` with only one.
    fn local_period(&self, i: usize, period: f32) -> f32 {
        if i + 1 < self.mark_len {
            (self.mark(i + 1) - self.mark(i)) as f32
        } else if i > 0 {
            (self.mark(i) - self.mark(i - 1)) as f32
        } else {
            period
        }
    }

    fn spawn(&mut self, grain: Grain) {
        let slot = match self.grains.iter().position(|g| !g.live) {
            Some(slot) => slot,
            None => {
                // Only a sudden large drop in period can get here: the old,
                // longer grains are still alive while short ones spawn fast.
                // The oldest grain is nearest its end and quietest.
                let (slot, _) = self
                    .grains
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.centre.total_cmp(&b.1.centre))
                    .expect("MAX_GRAINS > 0");
                slot
            }
        };
        self.grains[slot] = grain;
    }

    /// The shifted sample for output time o. Call once per sample after
    /// [`Self::push`], while running.
    pub fn next(&mut self, period: f32, ratio: f32) -> f32 {
        debug_assert!(self.running);
        let o = self.output_time();
        let of = o as f64;

        // Spawn grains whose windows now reach o. Each grain's length and the
        // step to the next synthesis mark follow the spacing of the marks
        // around its source, not the detector's period: the output's pitch
        // then follows the waveform's own periods. Tuned against the fixtures
        // at +4 (share within ±25¢): spacing by the detector's period scored
        // 0.36 / 0.52 / 0.63, spacing by the marks 0.72 / 0.81 / 0.85.
        while of >= self.next_synthesis - f64::from(period) {
            let centre = self.next_synthesis;
            let source = self.nearest_mark(centre);
            let local = self.local_period(source, period);
            let grain = Grain {
                centre,
                source: self.mark(source),
                half: local,
                live: true,
            };
            self.spawn(grain);
            self.next_synthesis += f64::from(local / ratio);
        }

        // Overlap-add.
        let mut acc = 0.0;
        let mut wsum = 0.0;
        for i in 0..MAX_GRAINS {
            let grain = self.grains[i];
            if !grain.live {
                continue;
            }
            let offset = of - grain.centre;
            let half = f64::from(grain.half);
            if offset.abs() < half {
                let w = (0.5 + 0.5 * (PI * offset / half).cos()) as f32;
                acc += w * self.read(grain.source as f64 + offset);
                wsum += w;
            }
            if offset >= half {
                self.grains[i].live = false;
            }
        }

        // Prune marks no grain can use any more, keeping the last one.
        let horizon = o - 2 * self.max_period;
        while self.mark_len > 1 && self.mark(0) < horizon {
            self.mark_head = (self.mark_head + 1) % MAX_MARKS;
            self.mark_len -= 1;
        }

        acc / f32::max(wsum, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A harmonic tone with harmonics at amplitude 1/k, peak ≈ 0.5.
    fn tone(sample_rate: f32, freq: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let phase =
                    std::f64::consts::TAU * f64::from(freq) * i as f64 / f64::from(sample_rate);
                let value: f64 = (1..=8).map(|k| (k as f64 * phase).sin() / k as f64).sum();
                (0.3 * value) as f32
            })
            .collect()
    }

    /// Feeds `signal` with a fixed period and ratio, starting once `o` has
    /// passed a full period. Returns the output (0.0 before the start) and
    /// the sample at which it started.
    fn run(psola: &mut Psola, signal: &[f32], period: f32, ratio: f32) -> (Vec<f32>, usize) {
        let start_at = (psola.latency + 2 * period.ceil() as i64) as usize;
        let output = signal
            .iter()
            .enumerate()
            .map(|(i, &x)| {
                if i == start_at {
                    psola.start(period);
                }
                psola.push(x, period);
                if psola.is_running() {
                    psola.next(period, ratio)
                } else {
                    0.0
                }
            })
            .collect();
        (output, start_at)
    }

    fn live_grains(psola: &Psola) -> usize {
        psola.grains.iter().filter(|g| g.live).count()
    }

    /// SNR in dB of `output` against `signal` delayed by the latency, from
    /// output sample `from` on.
    fn snr_db(psola: &Psola, signal: &[f32], output: &[f32], from: usize) -> f64 {
        let latency = psola.latency as usize;
        let (mut power, mut noise) = (0.0, 0.0);
        for i in from..output.len() {
            let expected = f64::from(signal[i - latency]);
            power += expected * expected;
            noise += (f64::from(output[i]) - expected).powi(2);
        }
        10.0 * (power / noise).log10()
    }

    #[test]
    fn latency_is_two_periods_of_the_lowest_pitch() {
        assert_eq!(latency_samples(44_100.0), 1103);
        assert_eq!(latency_samples(48_000.0), 1200);
        assert_eq!(latency_samples(96_000.0), 2400);
    }

    #[test]
    fn the_input_ring_covers_the_latency_and_two_long_periods() {
        for (sample_rate, len) in [(44_100.0, 4096), (48_000.0, 4096), (96_000.0, 8192)] {
            let psola = Psola::new(sample_rate);
            assert_eq!(psola.input.len(), len, "{sample_rate} Hz");
        }
    }

    #[test]
    fn marks_are_one_period_apart_on_peaks_of_one_sign() {
        for sample_rate in [44_100.0, 48_000.0] {
            let period = sample_rate / 220.0;
            let signal = tone(sample_rate, 220.0, sample_rate as usize / 2);
            let mut psola = Psola::new(sample_rate);
            run(&mut psola, &signal, period, 1.0);
            assert!(psola.mark_len > 5, "{} marks", psola.mark_len);
            for i in 0..psola.mark_len {
                let mark = psola.mark(i);
                let x = psola.polarity * psola.at(mark);
                assert!(x > 0.0, "mark {mark} is on the wrong side");
                assert!(x >= psola.polarity * psola.at(mark - 1), "mark {mark}");
                assert!(x >= psola.polarity * psola.at(mark + 1), "mark {mark}");
                if i > 0 {
                    let spacing = (mark - psola.mark(i - 1)) as f32;
                    assert!((spacing - period).abs() <= 1.0, "spacing {spacing}");
                }
            }
        }
    }

    #[test]
    fn ratio_one_reconstructs_the_input() {
        for sample_rate in [44_100.0, 48_000.0] {
            for freq in [110.0, 220.0, 440.0] {
                let period = sample_rate / freq;
                let signal = tone(sample_rate, freq, sample_rate as usize);
                let mut psola = Psola::new(sample_rate);
                let (output, start) = run(&mut psola, &signal, period, 1.0);
                let snr = snr_db(&psola, &signal, &output, start + 2 * period.ceil() as usize);
                println!("ratio 1, {freq} Hz at {sample_rate} Hz: SNR {snr:.1} dB");
                assert!(snr >= 20.0, "{freq} Hz at {sample_rate} Hz: {snr:.1} dB");
            }
        }
    }

    #[test]
    fn at_ratio_two_and_1_khz_at_most_five_grains_live() {
        for sample_rate in [44_100.0, 48_000.0] {
            let period = sample_rate / 1000.0;
            let signal = tone(sample_rate, 1000.0, sample_rate as usize / 4);
            let mut psola = Psola::new(sample_rate);
            let start = (psola.latency + 2 * period.ceil() as i64) as usize;
            let mut most = 0;
            for (i, &x) in signal.iter().enumerate() {
                if i == start {
                    psola.start(period);
                }
                psola.push(x, period);
                if psola.is_running() {
                    psola.next(period, 2.0);
                    most = most.max(live_grains(&psola));
                }
            }
            assert!((1..=5).contains(&most), "{most} grains at {sample_rate} Hz");
        }
    }

    #[test]
    fn non_finite_input_gives_finite_output() {
        let sample_rate = 48_000.0;
        let period = sample_rate / 220.0;
        let mut signal = tone(sample_rate, 220.0, 24_000);
        for (i, x) in signal.iter_mut().enumerate().skip(3000).step_by(97) {
            *x = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY][i % 3];
        }
        let mut psola = Psola::new(sample_rate);
        let (output, _) = run(&mut psola, &signal, period, 1.5);
        assert!(output.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn stop_then_start_works_without_stale_grains() {
        let sample_rate = 48_000.0;
        let period = sample_rate / 220.0;
        let signal = tone(sample_rate, 220.0, 48_000);
        let mut psola = Psola::new(sample_rate);
        let (_, _) = run(&mut psola, &signal[..12_000], period, 2.0);
        assert!(live_grains(&psola) > 0);
        psola.stop();
        assert!(!psola.is_running());
        assert_eq!(live_grains(&psola), 0);
        assert_eq!(psola.mark_len, 0);

        // Carry on feeding (stopped), then restart at ratio 1: the output is
        // the input again, with nothing left over from the ratio-2 grains.
        let restart = 20_000;
        let mut output = vec![0.0; signal.len()];
        for (i, &x) in signal.iter().enumerate().skip(12_000) {
            if i == restart {
                psola.start(period);
            }
            psola.push(x, period);
            if psola.is_running() {
                output[i] = psola.next(period, 1.0);
            }
        }
        let snr = snr_db(
            &psola,
            &signal,
            &output,
            restart + 2 * period.ceil() as usize,
        );
        assert!(snr >= 20.0, "{snr:.1} dB after restart");
    }

    #[test]
    fn reset_forgets_input_and_stops() {
        let sample_rate = 48_000.0;
        let period = sample_rate / 220.0;
        let signal = tone(sample_rate, 220.0, 12_000);
        let mut psola = Psola::new(sample_rate);
        run(&mut psola, &signal, period, 1.0);
        psola.reset();
        assert!(!psola.is_running());
        assert_eq!(psola.written, 0);
        assert!(psola.input.iter().all(|&x| x == 0.0));
    }
}
