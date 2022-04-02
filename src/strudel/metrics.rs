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
use prometheus::{Counter, CounterVec, Gauge, Histogram, HistogramOpts, Opts, Registry};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task;
use tokio::time::Instant;
use tracing::{span, Instrument, Level};

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
                        tracing::warn!(message = "unable to compute seconds since UNIX epoch", error = %e);
                    }
                }
            }
            Err(e) => {
                self.errors.with_label_values(&[e.kind().as_label()]).inc();
                tracing::error!(message = "unable to read sensor for metric collection", error = %e);
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

/// Adapter to allow metrics to be gathered in an async context.
///
/// Our metric collector takes a non-trivial amount of time to read the DHT22
/// sensor (100+ milliseconds) and so we can't call Registry::gather() from an
/// async context. This adapter runs the gather method in a Tokio thread pool
/// so that metrics can be collected from async request handlers.
#[derive(Debug)]
pub struct RegistryAdapter {
    registry: Registry,
}

impl RegistryAdapter {
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub async fn gather(&self) -> Vec<MetricFamily> {
        let registry = self.registry.clone();

        // Registry::gather() calls the collect() method of each registered collector. Our
        // collector blocks while reading the sensor via a GPIO pin. Since this code is called
        // in response to being scraped for metrics, it runs in the Hyper HTTP request path.
        // Run it in a thread pool below to avoid blocking the current future while the sensor
        // is being read (100+ milliseconds).
        match task::spawn_blocking(move || registry.gather())
            .instrument(span!(Level::DEBUG, "strudel_gather"))
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(message = "error gathering prometheus metrics", error = %e);
                Vec::new()
            }
        }
    }
}
