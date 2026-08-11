use std::process::ExitCode;

fn main() -> ExitCode {
    match aon_app::run_native() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
