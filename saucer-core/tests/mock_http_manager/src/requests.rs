use saucer_http_manager::{HttpError, HttpRequest, HttpResponse};

pub fn get<Msg: 'static>(
    url: impl Into<String>,
    returns: impl Fn(Result<HttpResponse, HttpError>) -> Msg + Send + Sync + 'static,
) -> HttpRequest<Msg> {
    saucer_http_manager::requests::get(url, returns)
}
