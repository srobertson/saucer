// Contrived mock to exercise saucer-core wiring; expected to change once a real
// saucer-http-manager with Elm-like `expect` APIs lands.
pub mod requests;
pub use requests::get;
use saucer_core::Router;
pub use saucer_http_manager::{HttpError, HttpMethod, HttpRequest, HttpResponse};

// === Effect Manager ===

pub struct HttpManager;

impl HttpManager {
    pub fn init() -> () {
        ()
    }

    pub fn on_effects<Msg: Send + 'static>(
        &self,
        router: &Router<Msg, ()>,
        state: (),
        effects: Vec<HttpRequest<Msg>>,
    ) -> () {
        for req in effects {
            let HttpRequest {
                method,
                url,
                returns,
                ..
            } = req;
            let body = match method {
                HttpMethod::Get => format!("GET {}", url),
                HttpMethod::Post => format!("POST {}", url),
            };
            let resp = HttpResponse {
                status: 200,
                headers: vec![],
                body: body.into_bytes(),
            };
            router.send_to_app(returns(Ok(resp)));
        }
        state
    }
}
