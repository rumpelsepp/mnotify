pub(crate) fn guess_mime(extension: &str) -> mime::Mime {
    match extension.to_lowercase().as_str() {
        "jpg" => mime::IMAGE_JPEG,
        "gif" => mime::IMAGE_GIF,
        "png" => mime::IMAGE_PNG,
        "pdf" => mime::APPLICATION_PDF,
        "opus" | "ogg" => "audio/ogg".parse().unwrap(),
        "mp3" => "audio/mp3".parse().unwrap(),
        _ => mime::APPLICATION_OCTET_STREAM,
    }
}
