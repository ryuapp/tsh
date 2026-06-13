use std::env;
use std::fs;
use std::path::PathBuf;

mod runtime;
mod shell;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let script_path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| "usage: tsh <script.js>".to_string())?;
    let source_code = fs::read_to_string(&script_path)
        .map_err(|err| format!("failed to read {}: {err}", script_path.display()))?;

    runtime::run_script(&source_code, &script_path.to_string_lossy())
}
