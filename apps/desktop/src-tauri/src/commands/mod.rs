pub mod activity;
pub mod capabilities;
pub mod duplicate;
pub mod library;
pub mod media;
pub mod portable;
pub mod relation;
pub mod service;
pub mod software;

pub use activity::*;
pub use capabilities::*;
pub use duplicate::*;
pub use library::*;
pub use media::*;
pub use portable::*;
pub use relation::*;
pub use service::*;
pub use software::*;

/// Existing canonical rows must carry an observed revision on desktop writes.
pub(crate) fn required_revision(
    value: Option<i64>,
    field: &str,
) -> Result<i64, crate::error::DesktopError> {
    value.filter(|revision| *revision > 0).ok_or_else(|| {
        crate::error::DesktopError::invalid_input(format!(
            "{field} is required and must be positive"
        ))
    })
}
