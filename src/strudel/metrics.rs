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

use crate::sensor::{Humidity, SensorError, TemperatureCelsius};
use prometheus_client::encoding::text::Encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Encode)]
struct ErrorsLabels {
    kind: String,
}

/// Collection of Prometheus metrics updated based on DHT22 sensor temperature and
/// humidity readings. Temperature in degrees celsius and relative humidity will be
/// emitted as gauges.
pub struct TemperatureMetrics {
    temperature: Gauge<f64>,
    humidity: Gauge<f64>,
    last_reading: Gauge<f64>,
    collections: Counter,
    errors: Family<ErrorsLabels, Counter>,
}

impl TemperatureMetrics {
    pub fn new(reg: &mut Registry) -> Self {
        let temperature = Gauge::<f64>::default();
        let humidity = Gauge::<f64>::default();
        let last_reading = Gauge::<f64>::default();
        let collections = Counter::default();
        let errors = Family::<ErrorsLabels, Counter>::default();

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
            "strudel_collections",
            "Number of attempted reads",
            Box::new(collections.clone()),
        );
        reg.register(
            "strudel_errors",
            "Number of failed reads by type",
            Box::new(errors.clone()),
        );

        Self {
            temperature,
            humidity,
            last_reading,
            collections,
            errors,
        }
    }

    pub fn update(&self, result: Result<(TemperatureCelsius, Humidity), SensorError>) {
        self.collections.inc();

        match result {
            Ok((temp, humidity)) => {
                self.temperature.set(temp.into());
                self.humidity.set(humidity.into());

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
