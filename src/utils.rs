use http::{header, StatusCode};
use http_service::Body;
use std::{
    cmp::min,
    path::{Path, PathBuf},
};

const MAX_BUFFER_SIZE: usize = 1024 * 1024 * 10;

/// Given request url path and base directory
///
/// Return `None` if the request might be a path traversal attack.
pub fn resolve_path(base: &Path, url_path: &str) -> Option<PathBuf> {
    let mut addition = PathBuf::new();
    // TODO work with urlencode
    // TODO With urlencode, component might contain '\', which could be different on Linux and Windows
    for component in url_path.split('/') {
        match component {
            "." => continue,
            ".." => {
                if !addition.pop() {
                    return None;
                }
            }
            _ => addition.push(component),
        }
    }
    Some(base.join(addition))
}

pub fn buffer_size(remain: u64) -> usize {
    min(remain as usize, MAX_BUFFER_SIZE)
}

pub fn not_found_response() -> http::Response<http_service::Body> {
    http::response::Builder::new()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, mime::TEXT_PLAIN_UTF_8.to_string())
        .body(Body::from("not found"))
        .unwrap()
}

pub fn server_error_response() -> http::Response<http_service::Body> {
    http::response::Builder::new()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(header::CONTENT_TYPE, mime::TEXT_PLAIN_UTF_8.to_string())
        .body(Body::from("not found"))
        .unwrap()
}
