// Pitemp - Temperature and humidity metrics exporter for Prometheus
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

use crate::metrics::MetricsExposition;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Method, Request, Response, StatusCode};
use prometheus::TEXT_FORMAT;
use std::sync::Arc;
use tracing::{event, Level};

pub struct RequestContext {
    exposition: MetricsExposition,
}

impl RequestContext {
    pub fn new(exposition: MetricsExposition) -> Self {
        RequestContext { exposition }
    }
}

pub async fn http_route(req: Request<Body>, context: Arc<RequestContext>) -> Result<Response<Body>, hyper::Error> {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();

    let res = match (&method, path.as_ref()) {
        (&Method::GET, "/metrics") => match context.exposition.encoded_text().await {
            Ok(buffer) => {
                event!(
                    Level::DEBUG,
                    message = "encoded prometheus metrics to text format",
                    num_bytes = buffer.len(),
                );

                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, TEXT_FORMAT)
                    .body(Body::from(buffer))
                    .unwrap()
            }
            Err(e) => {
                event!(
                    Level::ERROR,
                    message = "error scraping metrics",
                    error = %e,
                );

                http_status_no_body(StatusCode::INTERNAL_SERVER_ERROR)
            }
        },

        (_, "/metrics") => http_status_no_body(StatusCode::METHOD_NOT_ALLOWED),

        _ => http_status_no_body(StatusCode::NOT_FOUND),
    };

    Ok(res)
}

fn http_status_no_body(code: StatusCode) -> Response<Body> {
    Response::builder().status(code).body(Body::empty()).unwrap()
}
