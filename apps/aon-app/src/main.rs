use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = match arguments.as_slice() {
        [] => aon_app::run_native(),
        [command] if command == "stage0-product-probe" => {
            aon_app::run_native_stage0_product_probe()
        }
        _ => {
            eprintln!(
                "usage: aon-app\n       aon-app stage0-product-probe  # F5 current-input-only / F6 retained-state"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
