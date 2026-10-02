use std::io::{self, IsTerminal, Read};

use anyhow::anyhow;
use cli_prompts::{
    DisplayPrompt, prompts::AbortReason, prompts::Confirmation, style::ConfirmationStyle,
};

pub(crate) fn read_password() -> io::Result<String> {
    if io::stdin().is_terminal() {
        return rpassword::prompt_password("password: ");
    }
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(buf.trim_end_matches(['\r', '\n']).to_owned())
}

pub(crate) fn read_stdin_to_string() -> io::Result<String> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

/// Read a message body from stdin. A trailing newline (as `echo` adds it) is
/// dropped; on an interactive terminal a hint says how to finish the input.
pub(crate) fn read_message() -> io::Result<String> {
    if io::stdin().is_terminal() {
        eprintln!("Reading the message from stdin; finish with Ctrl-D.");
    }
    let mut buf = read_stdin_to_string()?;
    buf.truncate(buf.trim_end_matches(['\r', '\n']).len());
    Ok(buf)
}

/// Blocks on terminal input; call it from `spawn_blocking` in async code.
pub(crate) fn confirm(question: &str) -> anyhow::Result<bool> {
    Confirmation::new(question)
        .default_positive(false)
        .style(ConfirmationStyle::default())
        .display()
        .map_err(|e| match e {
            AbortReason::Interrupt => anyhow!("interrupted by user"),
            AbortReason::Error(e) => anyhow!(e),
        })
}
