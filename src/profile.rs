//! Internal profiling helpers.
//!
//! All instrumentation in this crate goes through the [`profile_span!`] macro
//! exported from the crate root. With the `tracing` Cargo feature **off** the
//! macro expands to nothing, so there is zero overhead in default builds.
//!
//! # Cargo features
//!
//! | Feature | Effect |
//! |---------|--------|
//! | (none)  | All [`profile_span!`] invocations compile to nothing. |
//! | `tracing` | Hot paths emit [`tracing`] spans. Consumers attach their own subscriber. |
//! | `tracy` | Implies `tracing`, plus a helper [`init_tracy`] that sets up a [`tracing-tracy`] subscriber for live profiling with a Tracy client. |
//!
//! # Usage
//!
//! ```rust,ignore
//! use voxel_framework::profile_span;
//!
//! pub fn hot_path() {
//!     profile_span!("voxel::mesh::greedy");
//!     // ... work that should appear in the profile ...
//! }
//! ```
//!
//! For Tracy:
//!
//! ```rust,ignore
//! fn main() {
//!     #[cfg(feature = "tracy")]
//!     voxel_framework::profile::init_tracy();
//!
//!     bevy::prelude::App::new()/* ... */.run();
//! }
//! ```
//!
//! [`tracing`]: https://docs.rs/tracing
//! [`tracing-tracy`]: https://docs.rs/tracing-tracy

#[cfg(feature = "tracing")]
pub use tracing::{Level, instrument, span};

/// Opens a profiling span scoped to the enclosing block.
///
/// Compiles to nothing when the `tracing` feature is disabled.
///
/// # Example
///
/// ```rust,ignore
/// use voxel_framework::profile_span;
///
/// fn generate() {
///     profile_span!("voxel::mesh::greedy");
///     // expensive work
/// }
/// ```
#[macro_export]
macro_rules! profile_span {
    ($name:expr) => {
        #[cfg(feature = "tracing")]
        let _voxel_profile_span =
            ::tracing::span!(::tracing::Level::INFO, $name).entered();
    };
}

/// Installs a `tracing` subscriber that forwards spans to a Tracy client.
///
/// Call this **once** from your binary's `main()` **before** constructing the
/// Bevy [`App`](bevy::app::App). The function is intentionally not auto-invoked
/// because most games already register their own subscriber and only one global
/// subscriber may be active at a time.
///
/// # Panics
///
/// Panics if a global default subscriber has already been installed.
#[cfg(feature = "tracy")]
pub fn init_tracy() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    tracing_subscriber::registry()
        .with(tracing_tracy::TracyLayer::default())
        .init();
}
