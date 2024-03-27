use http_digest_headers::{DigestHeader, DigestMethod};

pub fn generate_header(body: &[u8]) -> Result<String, ()> {
    let mut digest_header = DigestHeader::new()
        .with_method(DigestMethod::SHA256, body)
        .map(|h| format!("{h}"))
        .map_err(|_| ())?;

    // mastodon expects uppercase algo name
    if digest_header.starts_with("sha-") {
        digest_header.replace_range(..4, "SHA-");
    }
    // mastodon uses base64::alphabet::STANDARD, not base64::alphabet::URL_SAFE
    digest_header.replace_range(
        7..,
        &digest_header[7..].replace('-', "+").replace('_', "/")
    );

    Ok(digest_header)
}
