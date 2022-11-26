// Strudel - Temperature and humidity metrics exporter for Prometheus
//
// Copyright 2022 Nick Pillitteri
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

mod core;
mod dht22;
mod test;

pub use crate::sensor::core::{open_pin, DataPin, Humidity, SensorError, SensorErrorKind, TemperatureCelsius};
pub use crate::sensor::dht22::DHT22Sensor;
