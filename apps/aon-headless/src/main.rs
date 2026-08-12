use aon_headless::{
    ExperimentMaterializationSummary, HeadlessError, RunTrace, materialize_experiment_plan,
    run_replay_file, run_scenario,
};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match parse_request(std::env::args_os().skip(1).collect()).and_then(run_request) {
        Ok(output) => {
            print_output(output);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

enum Request {
    Scenario {
        scenario_path: PathBuf,
        ticks: u64,
    },
    Replay {
        replay_path: PathBuf,
    },
    ExperimentPlan {
        plan_path: PathBuf,
        output_directory: PathBuf,
    },
}

enum CommandOutput {
    Trace(RunTrace),
    Experiment(ExperimentMaterializationSummary),
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
        [command, plan_path, output_flag, output_directory]
            if command == "experiment-plan" && output_flag == "--output" =>
        {
            Ok(Request::ExperimentPlan {
                plan_path: PathBuf::from(plan_path),
                output_directory: PathBuf::from(output_directory),
            })
        }
        _ => Err(HeadlessError::Usage),
    }
}

fn run_request(request: Request) -> Result<CommandOutput, HeadlessError> {
    match request {
        Request::Scenario {
            scenario_path,
            ticks,
        } => run_scenario(scenario_path, ticks).map(CommandOutput::Trace),
        Request::Replay { replay_path } => run_replay_file(replay_path).map(CommandOutput::Trace),
        Request::ExperimentPlan {
            plan_path,
            output_directory,
        } => {
            materialize_experiment_plan(plan_path, output_directory).map(CommandOutput::Experiment)
        }
    }
}

fn print_output(output: CommandOutput) {
    match output {
        CommandOutput::Trace(trace) => {
            println!("scenario = {}", trace.scenario_id());
            println!("completed_ticks = {}", trace.completed_ticks());
            println!("state_hash = {}", trace.final_hash());
        }
        CommandOutput::Experiment(summary) => {
            println!("experiment_id = {}", summary.experiment_id());
            println!(
                "physical_scale_profiles = {}",
                summary.physical_scale_profile_count()
            );
            println!("runs = {}", summary.run_count());
            println!("manifest = {}", summary.manifest_path().display());
        }
    }
}
