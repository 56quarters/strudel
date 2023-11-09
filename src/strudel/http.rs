// Strudel - Temperature and humidity metrics exporter for Prometheus
//
// Copyright 2021-2022 Nick Pillitteri
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

use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use prometheus_client::encoding::text;
use prometheus_client::registry::Registry;
use std::sync::Arc;

const METRICS_TEXT: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";

#[derive(Debug)]
pub struct RequestState {
    pub registry: Registry,
}

pub async fn text_metrics_handler(State(state): State<Arc<RequestState>>) -> impl IntoResponse {
    let mut buf = String::new();
    let mut headers = HeaderMap::new();

    match text::encode(&mut buf, &state.registry) {
        Ok(_) => {
            tracing::debug!(message = "encoded prometheus metrics to text format", bytes = buf.len());
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(METRICS_TEXT));
            (StatusCode::OK, headers, buf.into_bytes())
        }
        Err(e) => {
            tracing::error!(message = "error encoding metrics to text format", error = %e);
            (StatusCode::INTERNAL_SERVER_ERROR, headers, Vec::new())
        }
    }
}
