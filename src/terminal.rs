use std::io::{self, Read};

use is_terminal::IsTerminal;
use prompts::{confirm::ConfirmPrompt, Prompt};

pub(crate) fn read_password() -> io::Result<String> {
    let mut res = String::new();
    let stdin = io::stdin();

    // TODO: use stdlib once stable:
    // https://doc.rust-lang.org/std/io/struct.Stdin.html#impl-IsTerminal-for-Stdin
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
    match ConfirmPrompt::new(question).run().await? {
        Some(res) => Ok(res),
        None => Ok(false),
    }
}
