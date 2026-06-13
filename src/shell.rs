use std::process::Command;

pub fn run(command: &str) -> Result<String, String> {
    let output = shell_command(command)
        .output()
        .map_err(|err| format!("failed to execute shell command `{command}`: {err}"))?;

    write_output(&output.stdout, &output.stderr)?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .map_err(|err| format!("command stdout was not valid UTF-8: {err}"))
    } else {
        Err(format!(
            "shell command `{command}` exited with {}",
            output.status
        ))
    }
}

#[cfg(not(test))]
fn write_output(stdout: &[u8], stderr: &[u8]) -> Result<(), String> {
    use std::io::{self, Write};

    io::stdout()
        .write_all(stdout)
        .map_err(|err| format!("failed to write command stdout: {err}"))?;
    io::stderr()
        .write_all(stderr)
        .map_err(|err| format!("failed to write command stderr: {err}"))?;

    Ok(())
}

#[cfg(test)]
fn write_output(_stdout: &[u8], _stderr: &[u8]) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", command]);
    cmd
}

#[cfg(not(windows))]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", command]);
    cmd
}
