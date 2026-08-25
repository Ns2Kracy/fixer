//! Safe preparation and execution of core output plans.

mod executor;
mod fingerprint;

pub use executor::{
    ExecutionError, ExecutionFailure, ExecutionPolicy, ExecutionReport, OperationReport,
    OperationStatus, OutputPlanExt, OverwritePolicy, PlacementMode, PreparedOutputPlan,
    ReflinkPolicy, plan_media_placement,
};
