use async_trait::async_trait;

use salvo_core::fs::{NamedFile, NamedFileBuilder};
use salvo_core::http::errors::*;
use salvo_core::routing::FlowCtrl;
use salvo_core::Handler;
use salvo_core::{Depot, Request, Response, Writer};

/// StaticFile
#[derive(Clone)]
pub struct StaticFile(NamedFileBuilder);

impl StaticFile {
    /// Create a new `StaticFile`.
    pub fn new(path: impl Into<String>) -> Self {
        StaticFile(NamedFile::builder(path.into().into()))
    }

    /// During the file chunk read, the maximum read size at one time will affect the
    /// access experience and the demand for server memory. Please set it according to your own situation.
    /// The default is 1M
    pub fn chunk_size(self, size: u64) -> Self {
        Self(self.0.with_buffer_size(size))
    }
}

#[async_trait]
impl Handler for StaticFile {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        match self.0.clone().build().await {
            Ok(file) => file.write(req, depot, res).await,
            Err(_) => {
                res.set_http_error(NotFound());
            }
        }
        ctrl.skip_reset();
    }
}
