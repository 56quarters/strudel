// Strudel - Temperature and humidity metrics exporter for Prometheus
//
// Copyright 2021 Nick Pillitteri
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.
//

use prometheus_client::encoding::text;
use prometheus_client::registry::Registry;
use std::sync::Arc;
use warp::http::header::CONTENT_TYPE;
use warp::http::{HeaderValue, StatusCode};
use warp::reply::Response;
use warp::{Filter, Rejection, Reply};

const TEXT_FORMAT: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";

/// Global stated shared between all HTTP requests via Arc.
pub struct RequestContext {
    registry: Registry,
}

impl RequestContext {
    pub fn new(registry: Registry) -> Self {
        RequestContext { registry }
    }
}

/// Create a warp Filter implementation that renders Prometheus metrics from
/// a registry in the text exposition format at the path `/metrics` for `GET`
/// requests. If an error is encountered, an HTTP 500 will be returned and the
/// error will be logged.
pub fn text_metrics(context: Arc<RequestContext>) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path("metrics").and(warp::filters::method::get()).map(move || {
        let context = context.clone();
        let mut buf = Vec::new();

        match text::encode(&mut buf, &context.registry) {
            Ok(_) => {
                tracing::debug!(message = "encoded prometheus metrics to text format",);
                let mut res = Response::new(buf.into());
                res.headers_mut()
                    .insert(CONTENT_TYPE, HeaderValue::from_static(TEXT_FORMAT));
                res
            }
            Err(e) => {
                tracing::error!(message = "error encoding metrics to text format", error = %e);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    })
}
