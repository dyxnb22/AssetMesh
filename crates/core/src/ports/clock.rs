//! Time source. Application services use an injectable clock so behavior is
//! testable; timestamps are always UTC.

use crate::domain::Timestamp;
use chrono::Utc;
use std::fmt::Debug;

pub trait Clock: Debug + Send + Sync {
    fn now(&self) -> Timestamp;
}

/// Production clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Utc::now()
    }
}
