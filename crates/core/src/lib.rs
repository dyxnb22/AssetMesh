//! AssetMesh core: domain, ports, and application layer.
//!
//! This crate is the headless kernel of AssetMesh (ADR 0001). It depends on
//! no UI framework, no HTTP stack, no database driver, and no OS APIs.
//! Infrastructure (SQLite, filesystem) lives in separate crates that
//! implement the ports defined here.

pub mod application;
pub mod domain;
pub mod error;
pub mod ports;

pub use application::{SharedClock, SharedIdGenerator};
pub use error::{AppError, AppResult};
