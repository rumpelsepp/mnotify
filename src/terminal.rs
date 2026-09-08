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

pub(crate) async fn confirm(question: &str) -> anyhow::Result<bool> {
    Confirmation::new(question)
        .default_positive(false)
        .style(ConfirmationStyle::default())
        .display()
        .map_err(|e| match e {
            AbortReason::Interrupt => anyhow!("interrupted by user"),
            AbortReason::Error(e) => anyhow!(e),
        })
}
