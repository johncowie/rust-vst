use std::sync::Arc;
use std::sync::atomic::Ordering;

use nice_plug::prelude::*;
use nice_plug_egui::{
    EguiEditor, EguiEditorState, EguiNiceSettings, RepaintNotifier, create_egui_editor,
};

use corral_dsp::gain;

use crate::editor::CorralEditor;

mod editor;

const MAX_GAIN_DB: f32 = 12.0;

/// Stereo gain. The −60 dB floor is treated as silence ("-inf").
pub struct Corral {
    params: Arc<CorralParams>,
    editor_state: Arc<EguiEditorState>,
    repaint_notifier: RepaintNotifier,
    /// The smoothed gain applied to the last frame, shown by the editor's readout.
    readout_gain: Arc<AtomicF32>,
    /// The last value stored in `readout_gain`, so the audio thread only
    /// stores and repaints when it changes.
    last_readout_gain: f32,
    /// Held until the host asks for the editor, which it does once.
    initial_editor: Option<CorralEditor>,
}

#[derive(Params)]
struct CorralParams {
    /// Linear gain. The ID is stored in saved sets and automation, so it must never change.
    #[id = "gain"]
    gain: FloatParam,
}

impl Default for Corral {
    fn default() -> Self {
        let params = Arc::new(CorralParams::default());
        let unity = util::db_to_gain(0.0);
        let readout_gain = Arc::new(AtomicF32::new(unity));

        Self {
            initial_editor: Some(CorralEditor::new(params.clone(), readout_gain.clone())),
            params,
            editor_state: editor::default_state(),
            repaint_notifier: RepaintNotifier::new(),
            readout_gain,
            last_readout_gain: unity,
        }
    }
}

impl Default for CorralParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::gain_range(gain::MIN_GAIN_DB, MAX_GAIN_DB),
            )
            // Logarithmic smoothing can't reach 0.0, which is why the range stops at −60 dB.
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(Arc::new(gain::format_gain))
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

    type Editor = EguiEditor<CorralEditor>;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_egui_editor(
            self.editor_state.clone(),
            self.repaint_notifier.clone(),
            EguiNiceSettings::new().with_tile(Self::NAME),
            self.initial_editor.take()?,
        )
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let mut last_gain = None;

        for channel_samples in buffer.iter_samples() {
            // One smoothed value per frame, shared by both channels.
            let gain = self.params.gain.smoothed.next();

            for sample in channel_samples {
                *sample = gain::apply_gain(*sample, gain);
            }
            last_gain = Some(gain);
        }

        // Share the gain with the readout once per buffer, only when it changes.
        // It's stored even while the window is closed, so the readout is right
        // when it opens. Both calls are single atomic stores.
        if let Some(gain) = last_gain
            && gain != self.last_readout_gain
        {
            self.last_readout_gain = gain;
            self.readout_gain.store(gain, Ordering::Relaxed);
            if self.editor_state.is_open() {
                self.repaint_notifier.request_repaint();
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
