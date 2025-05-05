use poem::{Route, handler, http::StatusCode, test::TestClient};

#[derive(Debug, thiserror::Error, poem_openapi_problemdetails::ApiProblemDetails)]
enum IndexError {
    #[error("")]
    #[oai_problemdetails(status = 404)]
    PlainError,
    #[error("")]
    #[oai_problemdetails(status = 422, ty = "https://example.com/probs/out-of-credit")]
    ErrorWithType,
    #[error("")]
    #[oai_problemdetails(status = 401, title = "Something went wrong")]
    ErrorWithTitle,
    #[error("")]
    #[oai_problemdetails(status = 403, detail = "Fill this number `{0}`")]
    ErrorWithDetail(u16),
}

#[handler]
fn plain_error() -> Result<(), IndexError> {
    Err(IndexError::PlainError)
}

#[handler]
fn error_with_type() -> Result<(), IndexError> {
    Err(IndexError::ErrorWithType)
}

#[handler]
fn error_with_title() -> Result<(), IndexError> {
    Err(IndexError::ErrorWithTitle)
}

#[handler]
fn error_with_detail() -> Result<(), IndexError> {
    Err(IndexError::ErrorWithDetail(42))
}

#[tokio::test]
async fn it_should_respond_with_the_correct_code() {
    let app = Route::new().at("/", plain_error);
    let cli = TestClient::new(app);

    let resp = cli.get("/").send().await;

    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn it_should_respond_with_content_type_problem_json() {
    let app = Route::new().at("/", plain_error);
    let cli = TestClient::new(app);

    let resp = cli.get("/").send().await;

    resp.assert_content_type("application/problem+json");
}

#[tokio::test]
async fn it_should_respond_with_about_blank_if_no_type_is_provided() {
    let app = Route::new().at("/", plain_error);
    let cli = TestClient::new(app);

    let mut resp = cli.get("/").send().await;

    let body = resp
        .0
        .take_body()
        .into_json::<serde_json::Value>()
        .await
        .expect("Failed to get json body");

    insta::assert_json_snapshot!(
        body,
        @r###"
            {
              "status": 404,
              "type": "about:blank"
            }
        "###
    );
}

#[tokio::test]
async fn it_should_respond_with_a_type_if_provided() {
    let app = Route::new().at("/", error_with_type);
    let cli = TestClient::new(app);

    let mut resp = cli.get("/").send().await;

    let body = resp
        .0
        .take_body()
        .into_json::<serde_json::Value>()
        .await
        .expect("Failed to get json body");

    insta::assert_json_snapshot!(
        body,
        @r###"
            {
              "status": 422,
              "type": "https://example.com/probs/out-of-credit"
            }
        "###
    );
}

#[tokio::test]
async fn it_should_respond_with_a_title_if_provided() {
    let app = Route::new().at("/", error_with_title);
    let cli = TestClient::new(app);

    let mut resp = cli.get("/").send().await;

    let body = resp
        .0
        .take_body()
        .into_json::<serde_json::Value>()
        .await
        .expect("Failed to get json body");

    insta::assert_json_snapshot!(
        body,
        @r###"
            {
              "status": 401,
              "title": "Something went wrong",
              "type": "about:blank"
            }
        "###
    );
}

#[tokio::test]
async fn it_should_respond_with_a_detail_if_provided() {
    let app = Route::new().at("/", error_with_detail);
    let cli = TestClient::new(app);

    let mut resp = cli.get("/").send().await;

    let body = resp
        .0
        .take_body()
        .into_json::<serde_json::Value>()
        .await
        .expect("Failed to get json body");

    insta::assert_json_snapshot!(
        body,
        @r###"
            {
              "detail": "Fill this number `42`",
              "status": 403,
              "type": "about:blank"
            }
        "###
    );
}
