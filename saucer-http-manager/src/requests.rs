use std::sync::Arc;

use crate::{HttpError, HttpMethod, HttpRequest, HttpResponse};

pub fn get<Msg>(
    url: impl Into<String>,
    returns: impl Fn(Result<HttpResponse, HttpError>) -> Msg + Send + Sync + 'static,
) -> HttpRequest<Msg> {
    HttpRequest {
        method: HttpMethod::Get,
        url: url.into(),
        headers: Vec::new(),
        body: None,
        returns: Arc::new(returns),
    }
}

pub fn post<Msg>(
    url: impl Into<String>,
    body: impl Into<Vec<u8>>,
    returns: impl Fn(Result<HttpResponse, HttpError>) -> Msg + Send + Sync + 'static,
) -> HttpRequest<Msg> {
    HttpRequest {
        method: HttpMethod::Post,
        url: url.into(),
        headers: Vec::new(),
        body: Some(body.into()),
        returns: Arc::new(returns),
    }
}
