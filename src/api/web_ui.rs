use axum::Router;
use std::sync::Arc;

use super::ServeState;

#[cfg(feature = "web-ui")]
mod embedded {
    use rust_embed::RustEmbed;

    #[derive(RustEmbed)]
    #[folder = "webui/out"]
    pub struct WebUiAssets;
}

pub fn routes() -> Router<Arc<ServeState>> {
    #[cfg(feature = "web-ui")]
    {
        use axum::{
            body::Body,
            http::{header, StatusCode, Uri},
            response::{IntoResponse, Response},
        };
        use embedded::WebUiAssets;

        fn try_serve(path: &str) -> Option<Response> {
            WebUiAssets::get(path).map(|content| {
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                let body = Body::from(content.data);
                let mut response = Response::new(body);
                response.headers_mut()
                    .insert(header::CONTENT_TYPE, mime.as_ref().parse().unwrap());
                response
            })
        }

        async fn serve_dashboard(uri: Uri) -> impl IntoResponse {
            let path = uri.path().trim_start_matches("/dashboard");
            let path = path.trim_start_matches('/');

            if path.is_empty() {
                return try_serve("dashboard.html")
                    .unwrap_or_else(|| (StatusCode::NOT_FOUND, "Not Found").into_response());
            }

            if let Some(response) = try_serve(path) {
                return response;
            }

            let dashboard_path = format!("dashboard/{path}");
            if let Some(response) = try_serve(&dashboard_path) {
                return response;
            }

            let dashboard_html = format!("dashboard/{path}.html");
            if let Some(response) = try_serve(&dashboard_html) {
                return response;
            }

            try_serve("dashboard.html")
                .unwrap_or_else(|| (StatusCode::NOT_FOUND, "Not Found").into_response())
        }

        Router::new().fallback(serve_dashboard)
    }

    #[cfg(not(feature = "web-ui"))]
    {
        Router::new()
    }
}