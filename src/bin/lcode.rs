use std::env;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let current_exe = match env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("failed to determine current executable path: {err}");
            return ExitCode::from(1);
        }
    };
    let launch_code_path = current_exe.with_file_name("launch-code");

    let status = match Command::new(&launch_code_path)
        .args(env::args_os().skip(1))
        .status()
    {
        Ok(status) => status,
        Err(err) => {
            eprintln!(
                "failed to execute launch-code at {}: {err}",
                launch_code_path.display()
            );
            return ExitCode::from(1);
        }
    };

    if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(status.code().unwrap_or(1) as u8)
    }
}
