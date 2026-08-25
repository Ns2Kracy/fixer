//! Filesystem-independent output planning and writer protocols.

mod plan;
mod writer;

pub use plan::{OutputOperation, OutputPlan, PlannedContent};
pub use writer::{PlanningError, WriteRequest, Writer};
