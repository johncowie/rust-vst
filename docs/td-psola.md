# TD-PSOLA in Corral

Corral shifts the pitch of a monophonic voice with TD-PSOLA
(time-domain pitch-synchronous overlap-add). This note explains how the
method works, how Corral implements it in realtime, how it compares with a
phase vocoder, and where it falls short. The code is in
`corral-dsp/src/psola.rs`, driven by `corral-dsp/src/chain.rs`.

## 1. What TD-PSOLA does

A voiced sound is a train of glottal pulses, one per period P, each pulse
exciting the vocal tract. TD-PSOLA works directly on those pulses.

- **Pitch marks.** The analysis step places one mark per period on the
  input, at a consistent point in each cycle. Corral uses the waveform's
  peaks.
- **Grains.** Around each analysis mark it cuts a grain two periods long
  (P either side), shaped by a Hann window. A grain holds one pulse and the
  vocal tract's response to it.
- **Overlap-add.** The synthesis step places grains at *synthesis marks*
  and adds them up. With synthesis marks spaced P apart, the grains
  overlap by half, their Hann windows sum to 1, and the input comes back
  unchanged.
- **Shifting.** To raise the pitch by a ratio r, synthesis marks are spaced
  P / r apart; to lower it, further apart. Each synthesis mark takes the
  grain from the analysis mark nearest in time. The output's pulses now
  repeat at the new period, so its pitch is r times the input's.
- **Duration doesn't change.** Synthesis marks run through time at the same
  pace as the input, so nothing is stretched. When pitch goes up there are
  more synthesis marks than analysis marks, so some grains are used twice;
  when pitch goes down, some are skipped.
- **Formants survive.** A grain's length follows the *analysis* period,
  not the ratio, so each grain keeps the vocal tract's response unchanged.
  The spectral envelope (the formants) therefore stays put, and only the
  spacing of the harmonics under it changes. The voice sounds like the same
  person singing higher, not a sped-up recording.

Raising the pitch by a major third (r ≈ 1.26):

```
input         |    P    |    P    |    P    |    P    |
analysis      a0        a1        a2        a3        a4
                ╱‾‾╲      ╱‾‾╲      ╱‾‾╲       grains: 2P, Hann, centred on a_k
output        s0      s1      s2      s3      s4      s5
              |  P/r  |  P/r  |  P/r  |  P/r  |  P/r  |
source        a0      a1      a2      a2      a3      a4   (nearest analysis mark;
                                                             a2 is used twice)
```

## 2. How Corral implements it

Corral runs sample by sample with a fixed delay, so every piece has to be
causal.

- **Period.** The YIN detector (see the README) reports a period every hop
  (256 samples at 44.1/48 kHz). `PeriodSmoother` (`period.rs`) turns it into
  the period the shifter uses:
  - moves of up to 100 ¢ are smoothed with a one-pole filter in log-period
    (coefficient 0.5 per hop);
  - larger jumps are note changes and are taken at once;
  - jumps within 150 ¢ of an octave are taken to be detector octave errors,
    and are ignored unless they persist for 3 hops.
- **Mark tracking.** When the shifter starts, the first mark is the
  largest |x| in the last period before the output point, and its sign sets
  the polarity. Every later mark is the largest `polarity · x` within ±P/4
  of the previous mark + P. A mark is confirmed only once its whole search
  window has arrived.
- **Grain spacing.** Each synthesis mark takes the confirmed analysis mark
  nearest to it. That grain's half-length is the *local mark spacing* (the
  distance to the next mark), and the next synthesis mark is placed that
  spacing ÷ r later. So the detector's period only steers mark tracking; the
  output's pitch follows the marks themselves. Spacing grains by the
  detector's period instead passed its per-hop error into the output and let
  grains drift out of step with the marks. It scored 0.36–0.63 on the
  fixtures, against 0.72–0.85 with local spacing.
- **Reading grains.** A grain reads the input at `source + (o − centre)`,
  with linear interpolation because synthesis marks fall between samples.
- **Normalisation.** Hann grains 2P long at spacing P / r sum to about r,
  so raising the pitch would make it louder. Each output sample is divided
  by `max(window sum, 1)`, which keeps the level for r ≥ 1 and never boosts
  the gaps between grains when r < 1.
- **Wet gate** (`wet_gate.rs`). Unvoiced input (breaths, consonants,
  silence) passes dry. The gate opens after 2 consecutive voiced hops and
  closes after 2 unvoiced ones, then crossfades between the delayed dry
  signal and the shifted one over 10 ms with equal power (cos/sin). While
  it is fully closed, or `Mix` is 0 %, the chain skips the mix arithmetic,
  so the output is the delayed input bit for bit.
- **Glide** (`shift.rs`). Each voice's shift in semitones follows its
  interval in the grid's row for the stabiliser's note (`stabiliser.rs`)
  through a one-pole filter in semitones, with `Glide` as its time
  constant. At voicing onset it jumps straight to the interval. It has no
  minimum ramp: a step in the ratio only moves where the next grain is
  placed, so even an instant change doesn't click. See the README's
  [Harmony](../README.md#harmony) section.
- **Stereo.** The detector and shifter work on the mono sum. The summed
  voices are added equally to both channels, and the dry path stays stereo.

### Several voices over one analysis

The harmony grid can sound up to 24 voices at once, each at its own
ratio. They all shift the same input, so the shifter is split in two
(`psola.rs`):

- **`Analysis`**, one per chain, holds everything that depends only on the
  input: the ring buffer of the mono sum, the pitch-mark tracker and the
  confirmed marks. It runs once per sample whatever the number of voices.
- **`Voice`**, one per sounding interval, holds only what depends on the
  ratio: its synthesis clock (where its next synthesis mark falls) and its
  active grains (5 at most at +12, with room for 8; each is a source mark,
  a centre and a half-length). It reads grains out of the shared ring.
- **Per sample**, the chain pushes the input into the analysis, runs every
  voice's `next`, then lets the analysis prune the marks no grain can use
  any more (older than o − 2·P_max, always keeping the last).
- **Starting.** When the wet gate opens, the analysis places its first mark
  and every voice live then starts synthesising from it. A voice added
  mid-note (a fade-in or an edit) starts at the latest confirmed mark at or
  before the output time, so it lines up with the marks already there.

A voice's own work per sample is a glide multiply-add, one `exp2` for its
ratio and the overlap-add of its few grains (a cosine and an interpolated
read each), plus a scan of the marks once per period when it places a
grain. Nothing in the analysis is repeated per voice, so each extra voice
costs about 13 µs per 512-frame block on Apple Silicon (22 µs under
Rosetta); 24 voices use 3.5 % (5.4 %) of real time at 48 kHz. See the
README's [CPU](../README.md#cpu) table. With a single voice the split
changed none of the shifter's printed test figures.

The voice pool (`voices.rs`) decides which voices sound and fades them in
and out over 20 ms; the fades multiply a voice's output, so the grains
themselves don't change.

### Latency

The chain's latency is fixed at L = ⌈2 · sr / 80⌉ samples: two periods of
the lowest detectable pitch (80 Hz). That is 1103 samples at 44.1 kHz,
1200 at 48 kHz and 2400 at 96 kHz, 25 ms in each case. It doesn't depend on
pitch, the grid, the number of voices, `Glide` or `Mix`, because in VST3 a change of latency restarts audio
in Live.

- With input time n, the output sample being made is o = n − L.
- A grain that starts at output time t_s − P needs its analysis mark then.
  A mark is confirmed once the input reaches mark + 1.25P, so the marks
  confirmed at that moment reach t_s − P + L − 1.25P.
- With L = 2·P_max, that is at least t_s − 0.25P at 80 Hz, and further
  ahead at higher pitches. So a grain always finds a mark within a quarter
  period of where it needs one. Any less latency would force grains near E2
  to use marks up to a whole period early.
- Reading the grain itself never limits latency: it reads at most P ahead
  of its mark, which is always ≤ n.
- **The detector lines up with the output.** The latest detection describes
  the centre of the 2048-sample window, n − 1024. The output is at n − 1103
  (44.1 kHz) or n − 1200 (48 kHz). The detection therefore describes audio
  79 or 176 samples ahead of the output, less than one hop. So voicing and
  period decisions are applied as soon as they arrive, with no extra delay.
  At 96 kHz, the window centre is n − 2048 and the output n − 2400, so the
  same holds.

## 3. Compared with a phase vocoder

A phase vocoder shifts pitch in the frequency domain. It takes short-time
FFTs, adjusts each bin's phase so that frames can be spaced differently,
resynthesises, then resamples to restore the duration.

| | TD-PSOLA (Corral) | Phase vocoder |
|---|---|---|
| **Latency** | About two periods of the lowest pitch: 25 ms here | At least one FFT frame, typically 2048–4096 samples (46–93 ms at 44.1 kHz), plus overlap |
| **Formants** | Kept, because grains keep the original period's spectral envelope | Move with the pitch ("chipmunk" voice) unless a separate envelope-correction stage is added |
| **Polyphony** | Monophonic only: it needs one period to mark | Handles chords and mixed sources |
| **Phasiness and transients** | No phasiness. Transients inside voiced audio are repeated or dropped along with their grain | Phasiness ("reverberant", smeared sound) unless phases are locked; transients smear across the frame |
| **Detector dependence** | Needs a reliable period and voicing decision; detector errors become audible glitches | Needs no pitch detector |
| **CPU** | A few grains per sample: one cosine, one interpolated read and a multiply-add each | FFT and inverse FFT per hop, plus per-bin phase work |

## 4. Why Corral uses TD-PSOLA

Corral harmonises a single voice or monophonic instrument, so TD-PSOLA's
main limitation, one input voice at a time, costs nothing. (Several
*output* voices are cheap: see
[Several voices over one analysis](#several-voices-over-one-analysis).) Its strengths are what
a vocal harmoniser needs:

- it keeps formants, so the harmony sounds like the same singer;
- its latency is low enough to monitor through;
- it has no phasiness on sustained notes.

Corral already runs a pitch detector to choose the harmony (Phase 5), so
PSOLA's need for one is met at no extra cost. No maintained Rust PSOLA crate
exists, and the available pitch-shift crates are phase-vocoder style, so
Corral has its own implementation.

## 5. Known artefacts and mitigations

- **Pulses and buzziness at large downward shifts.** At r < 1 the grains
  are spread further apart than they are long, leaving gaps, and at −12 the
  output can sound like a train of pulses. The window-sum floor of 1 stops
  the gaps' window tails being boosted to full level, which would add
  roughness without filling the gaps.
- **Level falls at large shifts.** The output samples the input's spectral
  envelope at the new harmonics. On the test tone (harmonics at 1/k), the
  level changes by −0.9 dB at +4, −1.8 dB at −5, −3.1 dB at +7, −3.5 dB at
  −12 and −6.3 dB at +12. At +12 the output keeps only the even harmonics,
  which carry 23 % of the power.
- **Mark jitter at high pitches.** Analysis marks sit on whole samples,
  and at 1 kHz a period is only 44–48 samples, so one sample of jitter is
  about 2 % (≈ 38 ¢) of a period for one grain. Synthesis marks keep their
  fractional position, and grains are read with linear interpolation, so
  the error doesn't accumulate. The median error stays within 1 ¢ on the
  test tones and the C3–C5 scale.
- **Detector errors.** Octave errors are ignored unless they persist for 3
  hops, and unvoiced hops pass the dry signal. A wrong period that the
  voicing gate accepts still produces a wrong-pitched or rough grain.
- **Onset delay.** The detector needs most of its 46 ms window to hold the
  new note before it voices a hop. The gate then waits 2 hops and fades over
  10 ms. The 25 ms latency hides part of this. At the output, the harmony
  starts fading in 11–23 ms after the note's onset (measured on synthesised
  tones at 82–440 Hz) and is fully in 10 ms later.
- **Transients in voiced audio.** A consonant or pick attack inside a voiced
  passage lands in one grain, which may be used twice (a stutter) or skipped
  (lost). Most consonants are unvoiced and pass through the gate dry.
- **Listening in Live.** Listening notes from Live 10 (low notes E2–A2,
  ±12, consonants and transients) are added here after the Phase 4 checks.

## 6. Sources

- E. Moulines and F. Charpentier, "Pitch-synchronous waveform processing
  techniques for text-to-speech synthesis using diphones", *Speech
  Communication* 9(5–6), 1990. The original TD-PSOLA paper.
- J. Driedger and M. Müller, "A Review of Time-Scale Modification of Music
  Signals", *Applied Sciences* 6(2), 2016:
  https://www.mdpi.com/2076-3417/6/2/57. Covers OLA, WSOLA and the phase
  vocoder, and their artefacts.
- J. Laroche and M. Dolson, "Improved phase vocoder time-scale modification
  of audio", *IEEE Transactions on Speech and Audio Processing* 7(3), 1999.
  Phase locking and phasiness.
- A. de Cheveigné and H. Kawahara, "YIN, a fundamental frequency estimator
  for speech and music", *Journal of the Acoustical Society of America*
  111(4), 2002. The pitch detector that drives the shifter.
- Kong, "PSOLA pitch shifting", Stanford EE264 final project, 2015:
  https://web.stanford.edu/class/ee264/projects/EE264_w2015_final_project_kong.pdf.
  PSOLA artefacts in practice.
