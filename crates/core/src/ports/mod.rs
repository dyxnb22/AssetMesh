//! Ports: interfaces the application depends on, implemented by
//! infrastructure adapters (hexagonal boundary, ADR 0001).
//!
//! Ports exist to isolate meaningful infrastructure boundaries — persistence,
//! time, identity generation, search. There is deliberately no interface for
//! every function.

pub mod clock;
pub mod ids;
pub mod repos;
pub mod search;
pub mod uow;

pub use clock::{Clock, SystemClock};
pub use ids::{IdGenerator, UuidV7Generator};
pub use repos::{
    ActivityReader, ActivityRepository, AssetFilter, AssetReader, AssetRepository,
    ExternalRefReader, ExternalRefRepository, LifecycleFilter, MediaFilter, MediaListRow,
    MediaReader, MediaRepository, MediaSort, TagReader, TagRepository,
};
pub use search::{SearchIndex, SearchReader};
pub use uow::{QueryUnitOfWork, UnitOfWork, UnitOfWorkFactory};
