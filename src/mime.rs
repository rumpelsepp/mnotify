use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail};
use tracing::warn;

pub fn guess_mime_file(path: impl AsRef<Path>) -> anyhow::Result<mime::Mime> {
    let child = Command::new("file")
        .arg("--mime")
        .arg(path.as_ref().as_os_str())
        .stdout(Stdio::piped())
        .spawn()?;

    let output = child.wait_with_output()?;

    if !output.status.success() {
        bail!("the file tool failed with exit code: {}", output.status);
    }

    let raw_output = String::from_utf8(output.stdout)?;
    let raw_mime = raw_output
        .split_once(':')
        .map(|x| x.1)
        .ok_or(anyhow!("bug? no mime in output"))?
        .trim();

    Ok(raw_mime.parse()?)
}

pub fn guess_mime_extension(path: impl AsRef<Path>) -> anyhow::Result<mime::Mime> {
    let extension = path.as_ref().extension();

    let res = match extension {
        None => mime::APPLICATION_OCTET_STREAM,
        Some(s) => match s.to_str().unwrap().to_lowercase().as_str() {
            "jpg" | "jpeg" => mime::IMAGE_JPEG,
            "gif" => mime::IMAGE_GIF,
            "png" => mime::IMAGE_PNG,
            "pdf" => mime::APPLICATION_PDF,
            "opus" | "ogg" => "audio/ogg".parse().unwrap(),
            "mp3" => "audio/mp3".parse().unwrap(),
            _ => mime::APPLICATION_OCTET_STREAM,
        },
    };

    Ok(res)
}

pub(crate) fn guess_mime(path: impl AsRef<Path>) -> anyhow::Result<mime::Mime> {
    match guess_mime_file(&path) {
        Ok(mime) => Ok(mime),
        Err(e) => {
            warn!("getting mimetype with `file` tool failed: {:?}", e);
            warn!("choosing mimetype from file extension");
            guess_mime_extension(&path)
        }
    }
}
