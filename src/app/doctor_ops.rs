use launch_code::state::StateStore;

use crate::cli::{DoctorArgs, DoctorCommands};
use crate::error::AppError;

pub(super) fn handle_doctor(store: &StateStore, args: &DoctorArgs) -> Result<(), AppError> {
    match &args.command {
        DoctorCommands::Debug(req) => super::doctor_debug_ops::handle_doctor_debug(store, req),
        DoctorCommands::Runtime(req) => super::doctor_runtime_ops::handle_doctor_runtime(req),
    }
}
