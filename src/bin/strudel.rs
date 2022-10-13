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

use clap::Parser;
use prometheus_client::registry::Registry;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{io, process};
use strudel::http::RequestContext;
use strudel::metrics::TemperatureMetrics;
use strudel::sensor::{open_pin, DHT22Sensor};
use tokio::signal::unix::{self, SignalKind};
use tokio::task;
use tracing::{Instrument, Level};

const DEFAULT_REFRESH_SECS: u64 = 30;
const DEFAULT_LOG_LEVEL: Level = Level::INFO;
const DEFAULT_BIND_ADDR: ([u8; 4], u16) = ([0, 0, 0, 0], 9781);

/// Expose temperature and humidity from a DHT22 sensor as Prometheus metrics
///
/// Read temperature and humidity data from a DHT22 sensor connected to a data pin
/// of a local machine, usually a Raspberry PI, and expose them as Prometheus
/// metrics. Several other metrics are emitted as well to help diagnose failures
/// reading the sensor.
///
/// The sensor must be connected to one of the General Purpose IO pins (GPIO). The
/// numbering of these pins (and how the pin number is provided to strudel) is based
/// on the Broadcom SOC channel.
#[derive(Debug, Parser)]
#[clap(name = "strudel", version = clap::crate_version ! ())]
struct StrudelApplication {
    /// BCM GPIO pin number the DHT22 sensor data line is connected to
    #[clap(long)]
    bcm_pin: u8,

    /// Read the sensor at this interval, in seconds
    #[clap(long, default_value_t = DEFAULT_REFRESH_SECS)]
    refresh_secs: u64,

    /// Logging verbosity. Allowed values are 'trace', 'debug', 'info', 'warn', and 'error'
    /// (case insensitive)
    #[clap(long, default_value_t = DEFAULT_LOG_LEVEL)]
    log_level: Level,

    /// Address to bind to. By default, strudel will bind to public address since
    /// the purpose is to expose metrics to an external system (Prometheus or another
    /// agent for ingestion)
    #[clap(long, default_value_t = DEFAULT_BIND_ADDR.into())]
    bind: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opts = StrudelApplication::parse();
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(opts.log_level)
            .finish(),
    )
    .expect("failed to set tracing subscriber");

    let pin = open_pin(opts.bcm_pin).unwrap_or_else(|e| {
        tracing::error!(message = "failed to initialize data pin", bcm_pin = opts.bcm_pin, error = %e);
        process::exit(1)
    });

    let mut reg = <Registry>::default();
    let metrics = TemperatureMetrics::new(&mut reg);
    let sensor = Arc::new(Mutex::new(DHT22Sensor::from_pin(pin)));

    // Periodically read from the sensor and update metrics based on the readings.
    task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(opts.refresh_secs));

        loop {
            let _ = interval.tick().await;
            let sensor_ref = sensor.clone();

            let res = task::spawn_blocking(move || {
                let mut s = sensor_ref.lock().unwrap();
                s.read()
            })
            .instrument(tracing::span!(Level::DEBUG, "sensor_read"))
            .await
            .unwrap();

            metrics.update(res);
        }
    });

    let context = Arc::new(RequestContext::new(reg));
    let handler = strudel::http::text_metrics(context);
    let (sock, server) = warp::serve(handler)
        .try_bind_with_graceful_shutdown(opts.bind, async {
            // Wait for either SIGTERM or SIGINT to shutdown
            tokio::select! {
                _ = sigterm() => {}
                _ = sigint() => {}
            }
        })
        .unwrap_or_else(|e| {
            tracing::error!(message = "error binding to address", address = %opts.bind, error = %e);
            process::exit(1)
        });

    tracing::info!(message = "server started", address = %sock);
    server.await;

    tracing::info!("server shutdown");
    Ok(())
}

/// Return after the first SIGTERM signal received by this process
async fn sigterm() -> io::Result<()> {
    unix::signal(SignalKind::terminate())?.recv().await;
    Ok(())
}

/// Return after the first SIGINT signal received by this process
async fn sigint() -> io::Result<()> {
    unix::signal(SignalKind::interrupt())?.recv().await;
    Ok(())
}
