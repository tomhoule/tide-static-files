#![feature(async_await)]
#![feature(await_macro)]
#![feature(futures_api)]

//! A helper for serving static files in the `tide` framework. It uses `tokio_fs` and assumes it
//! runs in the context of a tokio runtime (which is the case when you run tide with hyper, the
//! default http server implementation).
//!
//! ```
//! # use tide_static_files::StaticFiles;
//! #
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//!   let mut app = tide::App::new(());
//!
//!   app.at("/assets/*").get(StaticFiles::new("/var/lib/my-app/assets"));
//!
//!   # Ok(())
//! # }
//! ```

use http::StatusCode;
use http_service::Body;
use regex::Regex;
use std::path::{Path, PathBuf};
use tide::Response;

/// A struct that serves a directory.
///
/// ```
/// # use tide_static_files::StaticFiles;
/// #
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
///   let mut app = tide::App::new(());
///
///   app.at("/assets/*").get(StaticFiles::new("/var/lib/my-app/assets"));
///
///   # Ok(())
/// # }
///
/// ```
///
///
// The `Clone` bound can be dropped once we can use async/await in traits. For now the members of
// this struct all need to be owned by the returned futures. It may be more forward-compatible to
// clean members manually.
#[derive(Clone)]
pub struct StaticFiles {
    base: PathBuf,
    path_traversal_matcher: Regex,
}

impl StaticFiles {
    /// Create a StaticFiles handler for the directory at the provided path.
    pub fn new(path: &str) -> Self {
        StaticFiles {
            base: Path::new(path).into(),
            path_traversal_matcher: Self::path_traversal_regex(),
        }
    }

    async fn serve<'a>(&'a self, path: &'a str) -> Result<Response, Response> {
        use std::io::Read;

        if self.path_traversal_matcher.is_match(path) {
            return Ok(not_found_response());
        }

        let path = self.base.join(path);
        let file = await! { futures::compat::Compat01As03::new(tokio_fs::File::open(path)) };
        let mut file = file.map_err(|err| {
            log::warn!("Error reading file: {:?}", err);
            not_found_response()
        })?;
        let mut buf = Vec::new();

        file.read_to_end(&mut buf).expect("TODO: error handling");

        Ok(http::Response::new(buf.into()))
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

fn not_found_response() -> http::Response<http_service::Body> {
    let mut response = http::Response::new(Body::empty());
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

impl<S> tide::Endpoint<S, ()> for StaticFiles {
    type Fut = futures::future::FutureObj<'static, Response>;

    fn call(
        &self,
        _data: S,
        _req: tide::Request,
        params: Option<tide::RouteMatch<'_>>,
        _store: &tide::configuration::Store,
    ) -> Self::Fut {
        if let Some(ref matches) = params {
            if matches.vec.len() != 1 {
                panic!("multiple segments (TODO: better error message)");
            }

            let path = matches.vec[0].to_owned();

            // Necessary until async await in traits is available.
            let cloned = self.clone();

            futures::future::FutureObj::new(Box::new(
                async move {
                    let res = await! { cloned.serve(&path) };
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
    use http_service::HttpService;
    use std::fs::File;
    use std::io::Write;
    use tempfile::*;

    struct MockServer {
        backend: tide::Server<()>,
    }

    impl MockServer {
        fn simulate(
            &mut self,
            req: tide::Request,
        ) -> Result<(http::response::Parts, Vec<u8>), std::io::Error> {
            use futures::FutureExt;
            use futures::StreamExt;
            use futures::TryFutureExt;
            use tokio::runtime::current_thread::block_on_all;
            use tokio_threadpool::ThreadPool;
            let pool = ThreadPool::new();

            let mut connection = block_on_all(self.backend.connect().compat()).unwrap();
            block_on_all(pool.spawn_handle(self.backend.respond(&mut connection, req).compat()))
                .map(|res| {
                    let (head, body) = res.into_parts();
                    let body = block_on_all(
                        body.into_future()
                            .map(|r| -> Result<_, ()> { Ok(r) })
                            .compat(),
                    );
                    (
                        head,
                        body.unwrap()
                            .0
                            .map(|bytes| bytes.unwrap().to_vec())
                            .unwrap_or_else(|| Vec::new()),
                    )
                })
        }
    }

    fn test_app(mount_at: &str) -> (MockServer, TempDir) {
        let mut app = tide::App::new(());
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
        let (mut server, dir) = test_app("/static/*");

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
        let (mut server, dir) = test_app("/static/*");

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
        let (mut server, dir) = test_app("/static/*");

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
        let (mut server, dir) = test_app("/static/*");

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
    #[should_panic]
    fn static_files_with_multiple_wildcards_panics() {
        let (mut server, _dir) = test_app("/year/{}/static/*");

        let req = http::Request::builder()
            .uri("/year/2019/static/cats/meow.pdf")
            .body(http_service::Body::empty())
            .unwrap();

        server.simulate(req).unwrap();
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
}
