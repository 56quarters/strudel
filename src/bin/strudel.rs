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

use clap::{crate_version, Parser};
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use std::net::SocketAddr;
use std::process;
use std::sync::Arc;
use std::time::Instant;
use strudel::http::{http_route, RequestContext};
use strudel::metrics::{MetricsExposition, TemperatureMetrics};
use strudel::sensor::TemperatureReader;
use tracing::{event, span, Instrument, Level};

// const PIN_NUM: u8 = 17;

const DEFAULT_LOG_LEVEL: Level = Level::INFO;
const DEFAULT_BIND_ADDR: ([u8; 4], u16) = ([0, 0, 0, 0], 9781);

/// Expose temperature and humidity from a DHT22 sensor as Prometheus metrics
///
/// Blah blah blah, longer description goes here.
#[derive(Debug, Parser)]
#[clap(name = "strudel", version = crate_version!())]
struct PitempApplication {
    /// BCM GPIO pin number the DHT22 sensor data line is connected to
    #[clap(long)]
    bcm_pin: u8,

    /// Logging verbosity. Allowed values are 'trace', 'debug', 'info', 'warn', and 'error' (case insensitive)
    #[clap(long, default_value_t = DEFAULT_LOG_LEVEL)]
    log_level: Level,

    /// Address to bind to.
    #[clap(long, default_value_t = DEFAULT_BIND_ADDR.into())]
    bind: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opts = PitempApplication::parse();
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(opts.log_level)
            .finish(),
    )
    .expect("failed to set tracing subscriber");

    let startup = Instant::now();
    let reader = TemperatureReader::new(opts.bcm_pin).unwrap_or_else(|e| {
        event!(
            Level::ERROR,
            message = "failed to initialize sensor reader",
            bcm_pin = opts.bcm_pin,
            error = %e,
        );

        process::exit(1)
    });

    // Clone here since we're going to pass ownership of this to the MetricsExposition
    // instance created below. Cloning is relatively cheap since the state of the registry
    // is contained within an Arc.
    let reg = prometheus::default_registry().clone();
    reg.register(Box::new(TemperatureMetrics::new(reader)))
        .unwrap_or_else(|e| {
            event!(
                Level::ERROR,
                message = "failed to register sensor metric collector",
                error = %e,
            );

            process::exit(1)
        });

    let metrics = MetricsExposition::new(reg);
    let context = Arc::new(RequestContext::new(metrics));
    let service = make_service_fn(move |_| {
        let context = context.clone();

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                http_route(req, context.clone()).instrument(span!(Level::DEBUG, "strudel_request"))
            }))
        }
    });
    let server = Server::try_bind(&opts.bind).unwrap_or_else(|e| {
        event!(
            Level::ERROR,
            message = "server failed to start",
            address = %opts.bind,
            error = %e,
        );

        process::exit(1);
    });

    event!(
        Level::INFO,
        message = "server started",
        address = %opts.bind,
        bcm_pin = opts.bcm_pin,
    );

    server
        .serve(service)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    event!(
        Level::INFO,
        message = "server shutdown",
        runtime_secs = %startup.elapsed().as_secs(),
    );

    Ok(())
}
