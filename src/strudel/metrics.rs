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

use crate::sensor::TemperatureReader;
use prometheus::core::{Collector, Desc};
use prometheus::proto::MetricFamily;
use prometheus::{Counter, CounterVec, Encoder, Gauge, Histogram, HistogramOpts, Opts, Registry, TextEncoder};
use std::error::Error;
use std::fmt;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task;
use tokio::time::Instant;
use tracing::{event, span, Instrument, Level};

/// Prometheus Collector implementation that reads temperature and humidity from
/// a DHT22 sensor. Temperature in degrees celsius and relative humidity will be
/// emitted as gauges.
pub struct TemperatureMetrics {
    reader: Mutex<TemperatureReader>,
    temperature: Gauge,
    humidity: Gauge,
    last_reading: Gauge,
    collections: Counter,
    errors: CounterVec,
    timing: Histogram,
}

impl TemperatureMetrics {
    pub fn new(reader: TemperatureReader) -> Self {
        let temperature = Gauge::new("strudel_temperature_degrees", "Temperature in celsius").unwrap();

        let humidity = Gauge::new("strudel_relative_humidity", "Relative humidity (0-100)").unwrap();

        let last_reading = Gauge::new("strudel_last_read_timestamp", "Timestamp of last successful read").unwrap();

        let collections = Counter::new("strudel_collections_total", "Number of attempted reads").unwrap();

        let errors = CounterVec::new(Opts::new("strudel_errors_total", "Number of failed reads"), &["kind"]).unwrap();

        let timing = Histogram::with_opts(HistogramOpts::new(
            "strudel_read_timing_seconds",
            "Time taken to read the sensor in seconds",
        ))
        .unwrap();

        Self {
            reader: Mutex::new(reader),
            temperature,
            humidity,
            last_reading,
            collections,
            errors,
            timing,
        }
    }
}

impl Collector for TemperatureMetrics {
    fn desc(&self) -> Vec<&Desc> {
        let mut descs = Vec::new();
        descs.extend(self.temperature.desc());
        descs.extend(self.humidity.desc());
        descs.extend(self.last_reading.desc());
        descs.extend(self.collections.desc());
        descs.extend(self.errors.desc());
        descs.extend(self.timing.desc());
        descs
    }

    fn collect(&self) -> Vec<MetricFamily> {
        self.collections.inc();

        let start = Instant::now();
        let mut mfs = Vec::new();
        let mut reader = self.reader.lock().unwrap();

        match reader.read() {
            Ok((temp, humidity)) => {
                self.temperature.set(temp.into());
                self.humidity.set(humidity.into());
                self.timing.observe(start.elapsed().as_secs_f64());

                let now = SystemTime::now();
                match now.duration_since(UNIX_EPOCH) {
                    Ok(d) => self.last_reading.set(d.as_secs_f64()),
                    Err(e) => {
                        event!(
                            Level::WARN,
                            message = "unable to compute seconds since UNIX epoch",
                            error = %e
                        );
                    }
                }
            }
            Err(e) => {
                self.errors.with_label_values(&[e.kind().as_label()]).inc();
                event!(
                    Level::ERROR,
                    message = "unable to read sensor for metric collection",
                    error = %e,
                );
            }
        };

        mfs.extend(self.temperature.collect());
        mfs.extend(self.humidity.collect());
        mfs.extend(self.last_reading.collect());
        mfs.extend(self.collections.collect());
        mfs.extend(self.errors.collect());
        mfs.extend(self.timing.collect());
        mfs
    }
}

/// Error exposing Prometheus metrics in the text exposition format.
#[derive(Debug)]
pub struct ExpositionError {
    msg: &'static str,
    cause: Box<dyn Error + Send + Sync + 'static>,
}

impl ExpositionError {
    pub fn new<E>(msg: &'static str, cause: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        ExpositionError {
            msg,
            cause: Box::new(cause),
        }
    }
}

impl fmt::Display for ExpositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.msg, self.cause)
    }
}

impl Error for ExpositionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

/// Wrapper that exposes metrics from a Prometheus registry in the text exposition format.
///
/// This wrapper gathers all metrics from the registry in a separate thread, managed by the
/// tokio runtime in order to avoid blocking the future it is called from.
#[derive(Debug)]
pub struct MetricsExposition {
    registry: Registry,
}

impl MetricsExposition {
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }

    /// Collect all metrics from the registry and encode them in the Prometheus text exposition
    /// format, returning an error if metrics couldn't be collected or encoded for some reason.
    pub async fn encoded_text(&self) -> Result<Vec<u8>, ExpositionError> {
        let registry = self.registry.clone();

        // Registry::gather() calls the collect() method of each registered collector. Our
        // collector blocks while reading the sensor via a GPIO pin. Since this code is called
        // in response to being scraped for metrics, it runs in the Hyper HTTP request path.
        // Run it in a thread pool below to avoid blocking the current future while the sensor
        // is being read (100+ milliseconds).
        task::spawn_blocking(move || {
            let metric_families = registry.gather();
            let mut buffer = Vec::new();
            let encoder = TextEncoder::new();

            event!(
                Level::DEBUG,
                message = "encoding metric families to text exposition format",
                num_metrics = metric_families.len(),
            );

            encoder
                .encode(&metric_families, &mut buffer)
                .map_err(|e| ExpositionError::new("unable to encode Prometheus metrics", e))
                .map(|_| buffer)
        })
        .instrument(span!(Level::DEBUG, "strudel_gather"))
        .await
        .map_err(|e| ExpositionError::new("unable to gather Prometheus metrics", e))?
    }
}
