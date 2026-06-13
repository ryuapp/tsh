use std::process::Command;

use deno_task_shell::parser;
use deno_task_shell::parser::CommandInner;
use deno_task_shell::parser::PipelineInner;
use deno_task_shell::parser::Sequence;
use deno_task_shell::parser::SimpleCommand;
use deno_task_shell::parser::Word;
use deno_task_shell::parser::WordPart;

pub fn run(command: &str) -> Result<String, String> {
    let list =
        parser::parse(command).map_err(|err| format!("failed to parse shell command: {err}"))?;
    let simple_command = simple_command_from_list(&list)?;
    let args = simple_command
        .args
        .iter()
        .map(evaluate_word)
        .collect::<Result<Vec<_>, _>>()?;
    let (program, args) = args
        .split_first()
        .ok_or_else(|| "shell command is empty".to_string())?;

    let output = Command::new(program)
        .args(args)
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

fn simple_command_from_list(list: &parser::SequentialList) -> Result<&SimpleCommand, String> {
    if list.items.len() != 1 {
        return Err("only a single shell command is currently supported".to_string());
    }

    let item = &list.items[0];
    if item.is_async {
        return Err("async shell commands are not currently supported".to_string());
    }

    match &item.sequence {
        Sequence::Pipeline(pipeline) if !pipeline.negated => match &pipeline.inner {
            PipelineInner::Command(command) if command.redirect.is_none() => match &command.inner {
                CommandInner::Simple(simple) if simple.env_vars.is_empty() => Ok(simple),
                CommandInner::Simple(_) => Err(
                    "command-local environment variables are not currently supported".to_string(),
                ),
                CommandInner::Subshell(_) => {
                    Err("subshell commands are not currently supported".to_string())
                }
            },
            PipelineInner::Command(_) => {
                Err("shell redirection is not currently supported".to_string())
            }
            PipelineInner::PipeSequence(_) => {
                Err("shell pipelines are not currently supported".to_string())
            }
        },
        Sequence::Pipeline(_) => {
            Err("negated shell commands are not currently supported".to_string())
        }
        Sequence::BooleanList(_) => {
            Err("shell boolean lists are not currently supported".to_string())
        }
        Sequence::ShellVar(_) => {
            Err("shell variable assignments are not currently supported".to_string())
        }
    }
}

fn evaluate_word(word: &Word) -> Result<String, String> {
    evaluate_word_parts(word.parts())
}

fn evaluate_word_parts(parts: &[WordPart]) -> Result<String, String> {
    let mut value = String::new();
    for part in parts {
        match part {
            WordPart::Text(text) => value.push_str(text),
            WordPart::Quoted(parts) => value.push_str(&evaluate_word_parts(parts)?),
            WordPart::Variable(_) => {
                return Err("shell variable expansion is not currently supported".to_string());
            }
            WordPart::Tilde => return Err("tilde expansion is not currently supported".to_string()),
            WordPart::Command(_) => {
                return Err("command substitution is not currently supported".to_string());
            }
            WordPart::Brace(_) => {
                return Err("brace expansion is not currently supported".to_string());
            }
        }
    }
    Ok(value)
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
