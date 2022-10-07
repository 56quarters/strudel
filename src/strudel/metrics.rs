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
use prometheus_client::encoding::text::Encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::Histogram;
use prometheus_client::registry::Registry;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::task;
use tracing;

const BUCKETS: &[f64] = &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];

#[derive(Debug, Clone, Hash, PartialEq, Eq, Encode)]
struct ErrorsLabels {
    kind: String,
}

/// Prometheus Collector implementation that reads temperature and humidity from
/// a DHT22 sensor. Temperature in degrees celsius and relative humidity will be
/// emitted as gauges.
pub struct TemperatureMetrics {
    reader: Arc<Mutex<TemperatureReader>>,
    temperature: Gauge<f64>,
    humidity: Gauge<f64>,
    last_reading: Gauge<f64>,
    collections: Counter,
    errors: Family<ErrorsLabels, Counter>,
    timing: Histogram,
}

impl TemperatureMetrics {
    pub fn new(reg: &mut Registry, reader: TemperatureReader) -> Self {
        let temperature = Gauge::<f64>::default();
        let humidity = Gauge::<f64>::default();
        let last_reading = Gauge::<f64>::default();
        let collections = Counter::default();
        let errors = Family::<ErrorsLabels, Counter>::default();
        let timing = Histogram::new(BUCKETS.iter().copied());

        reg.register(
            "strudel_temperature_degrees",
            "Temperature in celsius",
            Box::new(temperature.clone()),
        );
        reg.register(
            "strudel_relative_humidity",
            "Relative humidity (0-100)",
            Box::new(humidity.clone()),
        );
        reg.register(
            "strudel_last_read_timestamp",
            "Timestamp of last successful read",
            Box::new(last_reading.clone()),
        );
        reg.register(
            "strudel_collections_total",
            "Number of attempted reads",
            Box::new(collections.clone()),
        );
        reg.register(
            "strudel_errors_total",
            "Number of failed reads",
            Box::new(errors.clone()),
        );
        reg.register(
            "strudel_read_timing_seconds",
            "Time taken to read the sensor in seconds",
            Box::new(timing.clone()),
        );

        Self {
            reader: Arc::new(Mutex::new(reader)),
            temperature,
            humidity,
            last_reading,
            collections,
            errors,
            timing,
        }
    }

    pub async fn collect(&self) {
        let start = Instant::now();
        let reader = self.reader.clone();

        // The sensor reader blocks while reading the sensor via a GPIO pin. Since this code
        // is called in response to being scraped for metrics, it runs in the Hyper HTTP request
        // path. Run it in a thread pool below to avoid blocking the current future while the sensor
        // is being read (100+ milliseconds).
        let res = task::spawn_blocking(move || {
            let mut r = reader.lock().unwrap();
            r.read()
        })
        .await
        .unwrap();

        self.collections.inc();

        match res {
            Ok((temp, humidity)) => {
                self.temperature.set(temp.into());
                self.humidity.set(humidity.into());
                self.timing.observe(start.elapsed().as_secs_f64());

                // If we can't get the number of seconds since the epoch, skip the update
                let _ = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| self.last_reading.set(d.as_secs_f64()));
            }
            Err(e) => {
                let labels = ErrorsLabels {
                    kind: e.kind().as_label().to_owned(),
                };

                self.errors.get_or_create(&labels).inc();
                tracing::error!(message = "unable to read sensor for metric collection", error = %e);
            }
        };
    }
}
