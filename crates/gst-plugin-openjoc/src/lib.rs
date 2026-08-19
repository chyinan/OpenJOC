//! Native GStreamer integration for OpenJOC.
//!
//! The actual plugin is intentionally behind the `gstreamer` feature so that
//! the normal OpenJOC workspace does not require a platform GStreamer SDK.
//! Build the loadable plugin with:
//!
//! ```text
//! cargo build -p gst-plugin-openjoc --release --features gstreamer
//! ```

#[cfg(feature = "gstreamer")]
mod plugin;

#[cfg(feature = "gstreamer")]
pub use plugin::register_static_plugin;

#[cfg(not(feature = "gstreamer"))]
/// Marker exported by the dependency-free workspace build.
pub const GSTREAMER_FEATURE_REQUIRED: &str =
    "build gst-plugin-openjoc with --features gstreamer to produce the plugin";
