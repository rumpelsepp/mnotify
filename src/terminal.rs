use std::io::{self, IsTerminal, Read};

use anyhow::anyhow;
use cli_prompts::{
    prompts::AbortReason, prompts::Confirmation, style::ConfirmationStyle, DisplayPrompt,
};

pub(crate) fn read_password() -> io::Result<String> {
    let mut res = String::new();
    let stdin = io::stdin();

    if stdin.is_terminal() {
        res = rpassword::prompt_password("password: ")?;
    } else {
        stdin.read_line(&mut res)?;
    }

    Ok(res)
}

pub(crate) fn read_stdin_to_string() -> io::Result<String> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

pub(crate) async fn confirm(question: &str) -> anyhow::Result<bool> {
    let prompt = Confirmation::new(question)
        .default_positive(false)
        .style(ConfirmationStyle::default());
    prompt.display().map_err(|e| match e {
        AbortReason::Interrupt => anyhow!("interrupted by user"),
        AbortReason::Error(e) => anyhow!(e),
    })
}
