// pattern: Functional Core

//! Streaming semantic inspection independent of the command-line frontend.
//! No PCM synthesis or rendering is performed by this crate.

mod aggregate;
mod container_metadata;
mod live;
mod model;
mod reader;
mod text;
pub use aggregate::InspectionAccumulator;
pub use live::{LiveInspectionObserver, LiveInspectionSnapshot};
pub use model::*;
pub use reader::{inspect_path, inspect_reader};
pub use text::format_summary;
