use aon_headless::{HeadlessError, RunTrace, run_replay_file, run_scenario};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match parse_request(std::env::args_os().skip(1).collect()).and_then(run_request) {
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

enum Request {
    Scenario { scenario_path: PathBuf, ticks: u64 },
    Replay { replay_path: PathBuf },
}

fn parse_request(arguments: Vec<OsString>) -> Result<Request, HeadlessError> {
    match arguments.as_slice() {
        [command, scenario_path, ticks_flag, ticks_value]
            if command == "scenario" && ticks_flag == "--ticks" =>
        {
            let ticks = ticks_value
                .to_str()
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or(HeadlessError::Usage)?;
            Ok(Request::Scenario {
                scenario_path: PathBuf::from(scenario_path),
                ticks,
            })
        }
        [command, replay_path] if command == "replay" => Ok(Request::Replay {
            replay_path: PathBuf::from(replay_path),
        }),
        _ => Err(HeadlessError::Usage),
    }
}

fn run_request(request: Request) -> Result<RunTrace, HeadlessError> {
    match request {
        Request::Scenario {
            scenario_path,
            ticks,
        } => run_scenario(scenario_path, ticks),
        Request::Replay { replay_path } => run_replay_file(replay_path),
    }
}
