use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};
use tracing::warn;

/// Guess a MIME type by asking the `file` tool.
fn guess_mime_file(path: &Path) -> anyhow::Result<mime::Mime> {
    let output = Command::new("file")
        .arg("--mime")
        .arg(path)
        .output()
        .context("could not run the `file` tool")?;

    if !output.status.success() {
        bail!("the `file` tool failed with {}", output.status);
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mime = stdout
        .split_once(':')
        .map(|(_, mime)| mime.trim())
        .context("no MIME type in `file` output")?;

    Ok(mime.parse()?)
}

/// Guess a MIME type from the file extension alone.
fn guess_mime_extension(path: &Path) -> mime::Mime {
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) => match ext.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => mime::IMAGE_JPEG,
            "gif" => mime::IMAGE_GIF,
            "png" => mime::IMAGE_PNG,
            "pdf" => mime::APPLICATION_PDF,
            "opus" | "ogg" => "audio/ogg".parse().unwrap(),
            "mp3" => "audio/mp3".parse().unwrap(),
            _ => mime::APPLICATION_OCTET_STREAM,
        },
        None => mime::APPLICATION_OCTET_STREAM,
    }
}

pub(crate) fn guess_mime(path: impl AsRef<Path>) -> anyhow::Result<mime::Mime> {
    let path = path.as_ref();
    match guess_mime_file(path) {
        Ok(mime) => Ok(mime),
        Err(e) => {
            warn!("`file` tool failed ({e}), guessing MIME type from the extension");
            Ok(guess_mime_extension(path))
        }
    }
}
