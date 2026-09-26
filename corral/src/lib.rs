use std::sync::Arc;

use nice_plug::prelude::*;

const MIN_GAIN_DB: f32 = -60.0;
const MAX_GAIN_DB: f32 = 12.0;

/// Stereo gain. The −60 dB floor is treated as silence ("-inf").
#[derive(Default)]
pub struct Corral {
    params: Arc<CorralParams>,
}

#[derive(Params)]
struct CorralParams {
    /// Linear gain. The ID is stored in saved sets and automation, so it must never change.
    #[id = "gain"]
    gain: FloatParam,
}

/// Gains at or below this are the floor and output silence. The smoother
/// lands exactly on its target, so the tolerance only absorbs float error.
fn silence_threshold() -> f32 {
    util::db_to_gain(MIN_GAIN_DB) * 1.0001
}

impl Default for CorralParams {
    fn default() -> Self {
        let gain_to_db = formatters::v2s_f32_gain_to_db(2);

        Self {
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::gain_range(MIN_GAIN_DB, MAX_GAIN_DB),
            )
            // Logarithmic smoothing can't reach 0.0, which is why the range stops at −60 dB.
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(Arc::new(move |gain| {
                if gain <= silence_threshold() {
                    String::from("-inf")
                } else {
                    gain_to_db(gain)
                }
            }))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

impl Plugin for Corral {
    const NAME: &'static str = "Corral";
    const VENDOR: &'static str = "John Cowie Del Corral";
    const URL: &'static str = "https://github.com/johncowie/rust-vst";
    const EMAIL: &'static str = "john.a.cowie@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;

    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let silence_threshold = silence_threshold();

        for channel_samples in buffer.iter_samples() {
            // One smoothed value per frame, shared by both channels.
            let gain = self.params.gain.smoothed.next();
            let silent = gain <= silence_threshold;

            for sample in channel_samples {
                *sample = if silent { 0.0 } else { *sample * gain };
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Corral {
    const CLAP_ID: &'static str = "com.johncowie.corral";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Auto-harmoniser");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo];
}

impl Vst3Plugin for Corral {
    const VST3_CLASS_ID: [u8; 16] = *b"JCowieCorral0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nice_export_clap!(Corral);
nice_export_vst3!(Corral);
