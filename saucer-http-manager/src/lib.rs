//! HTTP request types for saucer.
//!
//! This is a contrived stand-in to exercise saucer-core codegen/runtime.
//! The API will be replaced by a real saucer-http-manager that mirrors Elm-style
//! `Http.get { url, expect }` ergonomics; do not treat this surface as stable.

use saucer_core_macros::Request;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Clone, Request)]
pub struct HttpRequest<Msg> {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>, // None for GET
    pub returns: Arc<dyn Fn(Result<HttpResponse, HttpError>) -> Msg + Send + Sync>,
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct HttpError {
    pub message: String,
}

impl<Msg: 'static> HttpRequest<Msg> {
    pub fn map<Msg2>(
        self,
        f: impl Fn(Msg) -> Msg2 + Send + Sync + Clone + 'static,
    ) -> HttpRequest<Msg2> {
        let f = Arc::new(f);
        let returns = self.returns;
        HttpRequest {
            method: self.method,
            url: self.url,
            headers: self.headers,
            body: self.body,
            returns: Arc::new(move |r| f(returns(r))),
        }
    }
}
pub mod requests;
pub use requests::{get, post};
