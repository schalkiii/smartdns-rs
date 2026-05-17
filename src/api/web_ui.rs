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

        async fn serve_dashboard(uri: Uri) -> impl IntoResponse {
            let path = uri.path().trim_start_matches("/dashboard");
            let path = path.trim_start_matches('/');

            let path = if path.is_empty() {
                "dashboard.html"
            } else {
                path
            };

            match WebUiAssets::get(path) {
                Some(content) => {
                    let mime = mime_guess::from_path(path).first_or_octet_stream();
                    let body = Body::from(content.data);
                    let mut response = Response::new(body);
                    response.headers_mut().insert(
                        header::CONTENT_TYPE,
                        mime.as_ref().parse().unwrap(),
                    );
                    response
                }
                None => {
                    WebUiAssets::get("dashboard.html")
                        .map(|content| {
                            let mut response = Response::new(Body::from(content.data));
                            response.headers_mut().insert(
                                header::CONTENT_TYPE,
                                "text/html".parse().unwrap(),
                            );
                            response
                        })
                        .unwrap_or_else(|| {
                            (StatusCode::NOT_FOUND, "Not Found").into_response()
                        })
                }
            }
        }

        Router::new().fallback(serve_dashboard)
    }

    #[cfg(not(feature = "web-ui"))]
    {
        Router::new()
    }
}