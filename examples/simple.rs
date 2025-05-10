#![allow(dead_code)]

use poem::{Route, Server, listener::TcpListener};
use poem_openapi::{OpenApi, OpenApiService, payload::PlainText};

struct Api;

#[OpenApi]
impl Api {
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> Result<PlainText<&'static str>, IndexError> {
        Ok(PlainText("Hello World"))
    }
}

#[derive(Debug, thiserror::Error, poem_openapi_problemdetails::ApiProblemDetails)]
enum IndexError {
    /// The provided value was invalid
    #[error("Testings")]
    #[oai_problemdetails(
        status = 422,
        title = "The object passed failed to validate.",
        ty = "https://example.net/validation-error"
    )]
    InvalidValue(&'static str),
    /// Some unknown error occured
    #[error("Something really bad happened")]
    #[oai_problemdetails(status = 500)]
    InternalServerError,
}

#[tokio::main]
async fn main() {
    let api_service =
        OpenApiService::new(Api, "Hello World", "1.0").server("http://localhost:3000");

    let ui = api_service.swagger_ui();

    let app = Route::new().nest("/", api_service).nest("/docs", ui);

    Server::new(TcpListener::bind("127.0.0.1:8080"))
        .run(app)
        .await
        .expect("Failed to run server");
}
