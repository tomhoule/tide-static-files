#![feature(async_await)]

//! A helper for serving static files in the `tide` framework. It put file IO into a separate
//! thread pool, which means it wouldn't block `tide`'s runtime.
//!
//! TODO: example code

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;

mod file_request;
mod file_stream;
mod utils;

use crate::{file_request::FileRequest, utils::resolve_path};
use std::path::PathBuf;
use tide::{response::IntoResponse, Context, Response};

/// A struct that serves a directory.
///
/// TODO: example code
pub struct StaticFiles {
    base: PathBuf,
}

impl StaticFiles {
    /// Create a StaticFiles handler for the directory at the provided path.
    ///
    /// Return error if `base` doesn't exist or not directory or can't access
    pub fn new(base: impl Into<PathBuf>) -> Result<Self, failure::Error> {
        let base = base.into();
        if !base.canonicalize()?.is_dir() {
            bail!("given 'base' isn't a directory");
        }
        Ok(StaticFiles { base })
    }
}

impl<S: 'static> tide::Endpoint<S> for StaticFiles {
    type Fut = futures::future::BoxFuture<'static, Response>;

    fn call(&self, context: Context<S>) -> Self::Fut {
        let target_path = context
            .param::<String>("")
            .ok()
            .and_then(|url_path: String| resolve_path(&self.base, &url_path))
            .and_then(|path: PathBuf| path.canonicalize().ok());

        let file_request = FileRequest::new(target_path, context.headers());

        futures::FutureExt::boxed(async move { file_request.work().await.into_response() })
    }
}

// TODO unit test
