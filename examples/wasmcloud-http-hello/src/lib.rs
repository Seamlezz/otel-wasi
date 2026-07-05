mod bindings {
    use crate::Component;
    wit_bindgen::generate!({
        world: "component",
        path: "wit",
        generate_all,
    });
    export!(Component);
}

use bindings::exports::wasi::http::handler::Guest as Handler;
use bindings::wasi::http::types::{ErrorCode, Fields, Method, Request, Response};
use otel_wasi::ResultWithSlug;
use wit_bindgen::spawn_local;

struct Component;

impl Handler for Component {
    #[otel_wasi::wasi_instrument(
        service = "wasmcloud_http_hello",
        name = "handle",
        export,
        attributes(
            "http.route" = "/hello",
            "http.method" = "GET",
        )
    )]
    async fn handle(request: Request) -> Result<Response, otel_wasi::Error<ErrorCode>> {
        let method = request.get_method();
        let path_with_query = request.get_path_with_query().unwrap_or_default();
        let path = path_with_query.split('?').next().unwrap_or_default();

        otel_wasi::main_attribute!("http.route" = path.to_string());

        if path != "/hello" {
            otel_wasi::main_attribute!(
                "http.response.status_code" = 404i64,
                "http.hello.outcome" = "not_found",
            );
            return plain_response(404, b"not found\n".to_vec())
                .error_with_slug("http-hello-not-found");
        }

        if !matches!(method, Method::Get) {
            otel_wasi::main_attribute!(
                "http.response.status_code" = 405i64,
                "http.hello.outcome" = "method_not_allowed",
            );
            return plain_response(405, b"method not allowed\n".to_vec())
                .error_with_slug("http-hello-method-not-allowed");
        }

        otel_wasi::main_attribute!(
            "http.hello.outcome" = "success",
            "http.response.status_code" = 200i64,
        );
        plain_response(200, b"hello, world\n".to_vec())
            .error_with_slug("http-hello-response-build-failed")
    }
}

fn plain_response(status: u16, body_bytes: Vec<u8>) -> Result<Response, ErrorCode> {
    let headers = Fields::new();
    headers
        .set("content-type", &["text/plain".as_bytes().to_vec()])
        .map_err(|_| ErrorCode::InternalError(Some("failed to set content-type".to_string())))?;

    let (mut tx, rx) = bindings::wit_stream::new();
    let (trailers_tx, trailers_rx) = bindings::wit_future::new(|| todo!());

    spawn_local(async move {
        tx.write_all(body_bytes).await;
        drop(tx);
        let _ = trailers_tx.write(Ok(None)).await;
    });

    let (response, _result) = Response::new(headers, Some(rx), trailers_rx);
    response
        .set_status_code(status)
        .map_err(|()| ErrorCode::InternalError(Some("failed to set status".to_string())))?;

    Ok(response)
}
