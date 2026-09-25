use std::sync::Arc;

use nice_plug::prelude::*;

/// Stereo pass-through. Buffers are processed in place, so leaving them
/// untouched passes audio straight through.
#[derive(Default)]
pub struct Corral {
    params: Arc<CorralParams>,
}

#[derive(Params, Default)]
struct CorralParams {}

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
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
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
