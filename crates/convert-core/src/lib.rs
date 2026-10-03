//! Morfyl conversion core: pick the right engine for a file, build the command line,
//! run it with progress and cancellation. Has no UI or Tauri dependency, so it is unit-testable.

pub mod engines;
pub mod formats;
pub mod plan;
pub mod run;
pub mod util;

pub use engines::{Caps, Delivery, EngineId, Located, Locator, Source};
pub use formats::{Category, OutputChoice};
pub use plan::{plan, Job, Options, Plan, Quality, Toolbox};
pub use run::{run, Event};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{} is needed for this conversion but isn't installed", .0.name())]
    MissingEngine(EngineId),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    NotAvailable(String),
    #[error("{engine:?} failed: {message}")]
    Failed { engine: EngineId, message: String },
    #[error("Cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
