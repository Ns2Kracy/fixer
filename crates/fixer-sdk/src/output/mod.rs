//! Safe preparation and execution of core output plans.

mod executor;
mod fingerprint;

pub use executor::{
    ExecutionError, ExecutionFailure, ExecutionPolicy, ExecutionReport, OutputPlanExt,
    OverwritePolicy, PlacementMode, PreparedOutputPlan, ReflinkPolicy, plan_media_placement,
};
