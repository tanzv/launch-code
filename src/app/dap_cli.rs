mod basic;
mod control;
mod inspect;
mod shared;
mod subprocess_ops;

use launch_code::state::StateStore;

use crate::cli::{DapArgs, DapCommands};
use crate::error::AppError;

pub(super) fn handle_dap(store: &StateStore, args: &DapArgs) -> Result<(), AppError> {
    match &args.command {
        DapCommands::Request(req) => basic::handle_dap_request(store, req),
        DapCommands::Batch(req) => basic::handle_dap_batch(store, req),
        DapCommands::Breakpoints(req) => control::handle_dap_breakpoints(store, req),
        DapCommands::ExceptionBreakpoints(req) => {
            control::handle_dap_exception_breakpoints(store, req)
        }
        DapCommands::Evaluate(req) => control::handle_dap_evaluate(store, req),
        DapCommands::SetVariable(req) => control::handle_dap_set_variable(store, req),
        DapCommands::Continue(req) => control::handle_dap_continue(store, req),
        DapCommands::Pause(req) => control::handle_dap_pause(store, req),
        DapCommands::Next(req) => control::handle_dap_next(store, req),
        DapCommands::StepIn(req) => control::handle_dap_step_in(store, req),
        DapCommands::StepOut(req) => control::handle_dap_step_out(store, req),
        DapCommands::Disconnect(req) => control::handle_dap_disconnect(store, req),
        DapCommands::Terminate(req) => control::handle_dap_terminate(store, req),
        DapCommands::AdoptSubprocess(req) => {
            subprocess_ops::handle_dap_adopt_subprocess(store, req)
        }
        DapCommands::Events(req) => inspect::handle_dap_events(store, req),
        DapCommands::Threads(req) => inspect::handle_dap_threads(store, req),
        DapCommands::StackTrace(req) => inspect::handle_dap_stack_trace(store, req),
        DapCommands::Scopes(req) => inspect::handle_dap_scopes(store, req),
        DapCommands::Variables(req) => inspect::handle_dap_variables(store, req),
    }
}
