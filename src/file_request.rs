use crate::{
    file_stream::new_file_stream,
    utils::{not_found_response, server_error_response},
};
use http::HeaderMap;
use http_service::Body;
use std::{fs::File, ops::Range, path::PathBuf};
use tide::Response;

pub struct FileRequest {
    path: Option<PathBuf>,
}

impl FileRequest {
    pub fn new(path: Option<PathBuf>, _headers: &HeaderMap) -> Self {
        Self { path }
    }
}

impl FileRequest {
    pub async fn work(self) -> Response {
        let path = match self.path {
            None => return not_found_response(),
            Some(x) => x,
        };

        let file = match File::open(path) {
            Ok(x) => x,
            Err(error) => {
                error!("{}", error);
                return server_error_response();
            }
        };
        let size = match file.metadata() {
            Ok(x) => x.len(),
            Err(error) => {
                error!("{}", error);
                return server_error_response();
            }
        };

        Response::new(Body::from_stream(new_file_stream(
            file,
            Range {
                start: 0,
                end: size,
            },
        )))
    }
}
