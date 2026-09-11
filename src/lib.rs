//! CatchYT library root.
//!
//! The engine is UI-independent and unit-tested on its own; `app` and `theme`
//! hold the egui front-end. Splitting this into a library lets integration
//! tests exercise the engine directly.

pub mod app;
pub mod engine;
pub mod i18n;
pub mod settings;
pub mod theme;
