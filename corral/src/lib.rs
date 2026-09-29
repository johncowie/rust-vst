use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use nice_plug::prelude::*;
use nice_plug_egui::{
    EguiEditor, EguiEditorState, EguiNiceSettings, RepaintNotifier, create_egui_editor,
};

use corral_dsp::chain::{Chain, Settings};
use corral_dsp::grid::SharedGrid;
use corral_dsp::{note, shift};

use crate::editor::CorralEditor;

mod editor;

/// Pitch detection and a grid-driven multi-voice harmony.
pub struct Corral {
    params: Arc<CorralParams>,
    editor_state: Arc<EguiEditorState>,
    repaint_notifier: RepaintNotifier,
    /// The DSP chain. Rebuilt for the host's sample rate in `activate()`.
    chain: Chain,
    /// The last hop's note, packed with `note::pack`; `note::UNVOICED` when
    /// nothing is detected. Shown by the editor.
    note_readout: Arc<AtomicU32>,
    /// The last value stored in `note_readout`, so the audio thread only
    /// stores and repaints when it changes.
    last_note_readout: u32,
    /// The last hop's stable note, packed with `note::pack_midi`;
    /// `note::UNVOICED` while unvoiced. The editor derives the voices from
    /// it and the grid.
    stable_readout: Arc<AtomicU32>,
    /// The last value stored in `stable_readout`.
    last_stable_readout: u32,
    /// Toggled by the editor, copied by `process()` once per buffer. Not a
    /// parameter, so it isn't saved: every load starts empty.
    grid: Arc<SharedGrid>,
    /// Held until the host asks for the editor, which it does once.
    initial_editor: Option<CorralEditor>,
}

#[derive(Params)]
struct CorralParams {
    /// The glide's time constant in ms. Not smoothed: Chain glides the
    /// voices itself and caches the coefficient. The ID is stored in saved
    /// sets and automation, so it must never change.
    #[id = "glide"]
    glide: FloatParam,
    /// Dry/wet, 0.0–1.0, shown as a percentage. ID must never change.
    #[id = "mix"]
    mix: FloatParam,
}

impl Default for Corral {
    fn default() -> Self {
        let params = Arc::new(CorralParams::default());
        let note_readout = Arc::new(AtomicU32::new(note::UNVOICED));
        let stable_readout = Arc::new(AtomicU32::new(note::UNVOICED));
        let grid = Arc::new(SharedGrid::new());

        Self {
            initial_editor: Some(CorralEditor::new(
                params.clone(),
                grid.clone(),
                note_readout.clone(),
                stable_readout.clone(),
            )),
            params,
            editor_state: editor::default_state(),
            repaint_notifier: RepaintNotifier::new(),
            // A placeholder: the host always calls `activate()`, which
            // replaces it, before `process()`.
            chain: Chain::new(48_000.0),
            note_readout,
            last_note_readout: note::UNVOICED,
            stable_readout,
            last_stable_readout: note::UNVOICED,
            grid,
        }
    }
}

impl Default for CorralParams {
    fn default() -> Self {
        Self {
            glide: FloatParam::new(
                "Glide",
                1000.0 * shift::DEFAULT_GLIDE_S,
                FloatRange::Linear {
                    min: 0.0,
                    max: 1000.0 * shift::MAX_GLIDE_S,
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
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

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        // Allocates the detector's and shifter's buffers, off the audio thread.
        self.chain = Chain::new(buffer_config.sample_rate);
        // Fixed per sample rate: in VST3 a change restarts audio in Live.
        context.set_latency_samples(self.chain.latency() as u32);
        true
    }

    fn reset(&mut self) {
        self.chain.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let mut last_detection = None;
        // Read once per buffer: neither is smoothed. A toggle torn across
        // this copy only arrives a buffer later.
        let grid = self.grid.snapshot();
        let glide_s = self.params.glide.value() / 1000.0;

        for mut channel_samples in buffer.iter_samples() {
            // One smoothed value per frame, shared by both channels.
            let settings = Settings {
                mix: self.params.mix.smoothed.next(),
                grid,
                glide_s,
            };

            // Stereo in and out is the only layout, so the frame always has two channels.
            let mut frame = [0.0; 2];
            for (value, sample) in frame.iter_mut().zip(channel_samples.iter_mut()) {
                *value = *sample;
            }
            if let Some(detection) = self.chain.process_frame(&mut frame, settings) {
                last_detection = Some(detection);
            }
            for (sample, value) in channel_samples.iter_mut().zip(frame) {
                *sample = value;
            }
        }

        // Share readouts with the editor once per buffer, only when they change.
        // They're stored even while the window is closed, so it's right when it
        // opens. Each is a single atomic store. A buffer with no hop boundary
        // leaves the notes as they were.
        let mut changed = false;
        if let Some(detection) = last_detection {
            let packed = note::pack(
                detection
                    .voiced
                    .then(|| note::nearest_note(detection.f0_hz))
                    .flatten(),
            );
            if packed != self.last_note_readout {
                self.last_note_readout = packed;
                self.note_readout.store(packed, Ordering::Relaxed);
                changed = true;
            }
            // The chain keeps its note through unvoiced hops; blank it here.
            let packed = note::pack_midi(self.chain.stable_note().filter(|_| detection.voiced));
            if packed != self.last_stable_readout {
                self.last_stable_readout = packed;
                self.stable_readout.store(packed, Ordering::Relaxed);
                changed = true;
            }
        }
        if changed && self.editor_state.is_open() {
            self.repaint_notifier.request_repaint();
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
