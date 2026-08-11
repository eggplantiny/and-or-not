use aon_headless::{HeadlessError, run_scenario};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match parse_request(std::env::args_os().skip(1).collect())
        .and_then(|request| run_scenario(request.scenario_path, request.ticks))
    {
        Ok(trace) => {
            println!("scenario = {}", trace.scenario_id());
            println!("completed_ticks = {}", trace.completed_ticks());
            println!("state_hash = {}", trace.final_hash());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

struct Request {
    scenario_path: PathBuf,
    ticks: u64,
}

fn parse_request(arguments: Vec<OsString>) -> Result<Request, HeadlessError> {
    let [command, scenario_path, ticks_flag, ticks_value] = arguments.as_slice() else {
        return Err(HeadlessError::Usage);
    };

    if command != "scenario" || ticks_flag != "--ticks" {
        return Err(HeadlessError::Usage);
    }

    let ticks = ticks_value
        .to_str()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or(HeadlessError::Usage)?;

    Ok(Request {
        scenario_path: PathBuf::from(scenario_path),
        ticks,
    })
}
