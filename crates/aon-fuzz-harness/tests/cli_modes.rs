use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn all_mode_invokes_every_decoder_target_including_replay() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aon-fuzz-harness"))
        .arg("all")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fuzz replay CLI starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(b"{}")
        .expect("the bounded probe is written");
    let output = child.wait_with_output().expect("the fuzz replay CLI exits");
    assert!(
        output.status.success(),
        "all mode failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("CLI output is UTF-8");
    for prefix in [
        "decoder ",
        "replay ",
        "experiment ",
        "module ",
        "capacity-support ",
    ] {
        assert!(
            stdout.lines().any(|line| line.starts_with(prefix)),
            "all mode omitted `{prefix}` output:\n{stdout}"
        );
    }
}
