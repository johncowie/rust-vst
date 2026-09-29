//! Plain DSP for Corral. Nothing here depends on nice-plug, so the plugin,
//! the offline harness and the tests all run the same code.

pub mod chain;
pub mod delay;
pub mod eval;
pub mod grid;
pub mod harmony;
pub mod note;
pub mod period;
pub mod pitch;
pub mod psola;
pub mod shift;
pub mod stabiliser;
pub mod voices;
pub mod wet_gate;
