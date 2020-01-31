
//! A helper for serving static files in the `tide` framework. It uses `tokio_fs` and assumes it
//! runs in the context of a tokio runtime (which is the case when you run tide with hyper, the
//! default http server implementation).
//!
//! ```
//! # use tide_static_files::StaticFiles;
//! #
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//!   let mut app = tide::new();
//!
//!   app.at("/assets/*path").get(StaticFiles::new("/var/lib/my-app/assets"));
//!
//!   # Ok(())
//! # }
//! ```

use http::StatusCode;
use regex::Regex;
use std::path::{Path, PathBuf};
use tide::{Response, Request};

/// A struct that serves a directory.
///
/// ```
/// # use tide_static_files::StaticFiles;
/// #
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
///   let mut app = tide::new();
///
///   app.at("/assets/*path").get(StaticFiles::new("/var/lib/my-app/assets"));
///
///   # Ok(())
/// # }
///
/// ```
///
///
// The `Clone` impl can be dropped once we can use async/await in traits. For now the members of
// this struct all need to be owned by the returned futures. It may be more forward-compatible to
// copy members manually.
#[derive(Clone)]
pub struct StaticFiles {
    base: PathBuf,
    path_traversal_matcher: Regex,
}

use async_std::fs::File;
use async_std::io::BufReader;

impl StaticFiles {
    /// Create a StaticFiles handler for the directory at the provided path.
    pub fn new(path: &str) -> Self {
        StaticFiles {
            base: Path::new(path).into(),
            path_traversal_matcher: Self::path_traversal_regex(),
        }
    }

    async fn serve<'a>(&'a self, path: &'a str) -> Result<Response, Response> {
        if self.path_traversal_matcher.is_match(path) {
            return Ok(not_found_response());
        }

        let path = self.base.join(path);

        let mime = mime_guess::from_path(&path).first_or_text_plain();

        let file = BufReader::new(File::open(path).await
            .map_err(|err| {
                log::warn!("Error reading file: {:?}", err);
                not_found_response()
            })?);

        let resp = Response::new(StatusCode::OK.into())
            .body(file)
            .set_mime(mime);

        Ok(resp)
    }

    /// https://github.com/SergioBenitez/Rocket/blob/f857f81d9c156cbb6f8b24be173dbda0cb0504a0/core/http/src/uri/segments.rs#L65
    /// was used as a reference
    fn path_traversal_regex() -> Regex {
        Regex::new(
            r#"
            (?x) # ignore whitespace and allow comments in the regex
            # Double dots
            (\.\.[/\\]) |
            # hidden files
            (/\.) |
            (^\.) |
            # initial *
            (^\*) |
            # \\ (windows)
            (\\\\)
            "#,
        )
        .unwrap()
    }
}

fn not_found_response() -> Response {
    let response = Response::new(StatusCode::NOT_FOUND.into());
    response
}

impl<S: 'static> tide::Endpoint<S> for StaticFiles {

    type Fut = futures::future::FutureObj<'static, Response>;

    fn call(
        &self,
        req: Request<S>,
    ) -> Self::Fut {
        if let Ok(path) = req.param::<String>("path") {
            let path = path.to_owned();

            // Necessary until async await in traits is available.
            let cloned = self.clone();

            futures::future::FutureObj::new(Box::new(
                async move {
                    let res = cloned.serve(&path).await;
                    match res {
                        Ok(response) => response,
                        Err(response) => response,
                    }
                },
            ))
        } else {
            unimplemented!("static file index")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_service::{HttpService, };
    use std::fs::File;
    use std::io::Write;
    use tempfile::*;
    
    use async_std::io::ReadExt;

    struct MockServer {
        backend: tide::server::Service<()>,
    }

    impl MockServer {
        fn simulate(&mut self, req: http_service::Request) 
            -> Result<(http::response::Parts, Vec<u8>), std::io::Error> {
            use async_std::*;

            let mut connection = task::block_on(self.backend.connect()).unwrap();
            let res = task::block_on(self.backend.respond(&mut connection, req))?;
                    let (head, mut body) = res.into_parts();
                    let mut body_vec = Vec::new();
                    task::block_on(body.read_to_end(&mut body_vec)).unwrap(); 
                    Ok((
                        head,
                        body_vec
                    ))
        }
    }

    fn test_app(mount_at: &str) -> (MockServer, TempDir) {
        let mut app = tide::new();
        let temp_dir = TempDir::new().unwrap();
        let endpoint = StaticFiles::new(&format!("{}", temp_dir.path().to_string_lossy()));

        app.at(mount_at).get(endpoint);

        (
            MockServer {
                backend: app.into_http_service(),
            },
            temp_dir,
        )
    }

    #[test]
    fn static_files_simplest_case() {
        let (mut server, dir) = test_app("/static/*path");

        let file_path = dir.path().join("meow.pdf");
        let mut file = File::create(file_path).unwrap();

        write!(file, "{}", "says the cat").unwrap();

        let req = http::Request::builder()
            .uri("/static/meow.pdf")
            .body(http_service::Body::empty())
            .unwrap();

        let (head, body) = server.simulate(req).unwrap();

        assert_eq!(head.status, 200);

        assert_eq!(String::from_utf8(body).unwrap(), "says the cat");
    }

    #[test]
    fn static_files_subdirectory() {
        let (mut server, dir) = test_app("/static/*path");

        std::fs::create_dir(dir.path().join("cats")).ok();
        let file_path = dir.path().join("cats/meow.pdf");
        let mut file = File::create(file_path).unwrap();

        write!(file, "{}", "says the cat").unwrap();

        let req = http::Request::builder()
            .uri("/static/cats/meow.pdf")
            .body(http_service::Body::empty())
            .unwrap();

        let (head, body) = server.simulate(req).unwrap();

        assert_eq!(head.status, 200);

        assert_eq!(String::from_utf8(body).unwrap(), "says the cat");
    }

    #[test]
    fn path_traversal_is_not_allowed() {
        let (mut server, dir) = test_app("/static/*path");

        std::fs::create_dir(dir.path().join("cats")).ok();
        let file_path = dir.path().join("meow.pdf");
        let mut file = File::create(file_path).unwrap();

        write!(file, "{}", "says the cat").unwrap();

        let req = http::Request::builder()
            .uri("/static/cats/../meow.pdf")
            .body(http_service::Body::empty())
            .unwrap();

        let (head, body) = server.simulate(req).unwrap();

        assert_eq!(head.status, 404);

        assert_eq!(String::from_utf8(body).unwrap(), "");
    }

    #[test]
    fn static_files_urlencoded_is_ignored() {
        let (mut server, dir) = test_app("/static/*path");

        std::fs::create_dir(dir.path().join("cats")).ok();
        let file_path = dir.path().join("cats/meow.pdf");
        let mut file = File::create(file_path).unwrap();

        write!(file, "{}", "says the cat").unwrap();

        let req = http::Request::builder()
            .uri("/static/cats%2F/meow.pdf")
            .body(http_service::Body::empty())
            .unwrap();

        let (head, body) = server.simulate(req).unwrap();

        assert_eq!(head.status, 404);

        assert_eq!(String::from_utf8(body).unwrap(), "");
    }

    #[test]
    fn path_traversal_regex_works() {
        let regex = StaticFiles::path_traversal_regex();

        // ..
        assert!(regex.is_match("../"));
        assert!(regex.is_match(".."));
        assert!(regex.is_match("/../"));

        // hidden files
        assert!(regex.is_match("/ab/.c"));

        // *
        assert!(regex.is_match("*/ab/c"));

        // \\
        assert!(regex.is_match("*/ab/c/\\e/f"));
    }

    #[test]
    fn test_correct_mime_html () {
        let (mut server, dir) = test_app("/static/*path");

        std::fs::create_dir(dir.path().join("cats")).ok();
        let file_path = dir.path().join("cats/meow.html");
        let mut file = File::create(file_path).unwrap();

        write!(file, "{}", "<html>says the cat</html>").unwrap();

        let req = http::Request::builder()
            .uri("/static/cats/meow.html")
            .body(http_service::Body::empty())
            .unwrap();

        let (head, body) = server.simulate(req).unwrap();

        assert_eq!(head.status, 200);

        assert_eq!(String::from_utf8(body).unwrap(), "<html>says the cat</html>");
        use http::header::*;
        assert_eq!(head.headers[CONTENT_TYPE], "text/html");

    }
}
