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

use crate::sensor::TemperatureReader;
use prometheus::core::{Collector, Desc};
use prometheus::proto::MetricFamily;
use prometheus::{Counter, CounterVec, Encoder, Gauge, Opts, Registry, TextEncoder};
use std::error::Error;
use std::fmt;
use std::sync::Mutex;
use tokio::task;
use tracing::{event, span, Instrument, Level};

/// Prometheus Collector implementation that reads temperature and humidity from
/// a DHT22 sensor. Temperature in degrees celsius and relative humidity will be
/// emitted as gauges.
pub struct TemperatureMetrics {
    reader: Mutex<TemperatureReader>,
    temperature: Gauge,
    humidity: Gauge,
    collections: Counter,
    errors: CounterVec,
}

impl TemperatureMetrics {
    pub fn new(reader: TemperatureReader) -> Self {
        let temperature = Gauge::new("pitemp_temperature_celsius", "Temperature in celsius")
            .expect("unable to declare temperature gauge");

        let humidity = Gauge::new("pitemp_relative_humidity", "Relative humidity (0-100)")
            .expect("unable to declare humidity gauge");

        let collections = Counter::new("pitemp_collections_total", "Number of attempted reads")
            .expect("unable to declare collections counter");

        let errors = CounterVec::new(Opts::new("pitemp_errors_total", "Number of failed reads"), &["kind"])
            .expect("unable to declare errors counter");

        Self {
            reader: Mutex::new(reader),
            temperature,
            humidity,
            collections,
            errors,
        }
    }
}

impl Collector for TemperatureMetrics {
    fn desc(&self) -> Vec<&Desc> {
        let mut descs = Vec::new();
        descs.extend(self.temperature.desc());
        descs.extend(self.humidity.desc());
        descs.extend(self.collections.desc());
        descs.extend(self.errors.desc());
        descs
    }

    fn collect(&self) -> Vec<MetricFamily> {
        self.collections.inc();
        let mut mfs = Vec::new();
        let mut reader = self.reader.lock().unwrap();

        match reader.read() {
            Ok((temp, humidity)) => {
                self.temperature.set(temp.into());
                self.humidity.set(humidity.into());
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
        mfs.extend(self.collections.collect());
        mfs.extend(self.errors.collect());
        mfs
    }
}

/// Error exposing Prometheus metrics in the text exposition format.
#[derive(Debug)]
pub enum ExpositionError {
    Runtime(&'static str, Box<dyn Error + Send + Sync + 'static>),
    Encoding(&'static str, Box<dyn Error + Send + Sync + 'static>),
}

impl fmt::Display for ExpositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpositionError::Runtime(msg, ref e) => write!(f, "{}: {}", msg, e),
            ExpositionError::Encoding(msg, ref e) => write!(f, "{}: {}", msg, e),
        }
    }
}

impl Error for ExpositionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ExpositionError::Runtime(_, ref e) => Some(e.as_ref()),
            ExpositionError::Encoding(_, ref e) => Some(e.as_ref()),
        }
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
                .map_err(|e| ExpositionError::Encoding("unable to encode Prometheus metrics", Box::new(e)))
                .map(|_| buffer)
        })
        .instrument(span!(Level::DEBUG, "pitemp_gather"))
        .await
        .map_err(|e| ExpositionError::Runtime("unable to gather Prometheus metrics", Box::new(e)))?
    }
}
