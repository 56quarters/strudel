use crate::sensor::TemperatureReader;
use prometheus::core::{Collector, Desc};
use prometheus::proto::MetricFamily;
use prometheus::{Counter, Encoder, Gauge, Registry, TextEncoder};
use std::error::Error;
use std::fmt;
use std::sync::Mutex;
use tracing::{event, Level};

pub struct TemperatureMetrics {
    reader: Mutex<TemperatureReader>,
    temperature: Gauge,
    humidity: Gauge,
    collections: Counter,
    errors: Counter,
}

impl TemperatureMetrics {
    pub fn new(reader: TemperatureReader) -> Self {
        let temperature = Gauge::new("pitemp_temperature_celsius", "Temperature in celsius")
            .expect("unable to declare temperature gauge");

        let humidity = Gauge::new("pitemp_relative_humidity", "Relative humidity (0-100)")
            .expect("unable to declare humidity gauge");

        let collections = Counter::new("pitemp_collections_total", "Number of attempted reads")
            .expect("unable to declare collections counter");

        let errors =
            Counter::new("pitemp_errors_total", "Number of failed reads").expect("unable to declare errors counter");

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
                self.errors.inc();
                event!(
                    Level::ERROR,
                    message = "unable to read sensor for metric collection",
                    error = %e,
                );
            }
        };

        mfs.extend(self.temperature.collect());
        mfs.extend(self.humidity.collect());
        mfs
    }
}

///
///
///
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

///
///
///
#[derive(Debug)]
pub struct MetricsExposition {
    registry: Registry,
}

impl MetricsExposition {
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub async fn encoded_text(&self) -> Result<Vec<u8>, ExpositionError> {
        let registry = self.registry.clone();

        // TODO(56quarters): Explain this, .gather() blocks on reading the sensor so we
        //  need to run it in a thread pool to avoid tying up tokio and preventing it from
        //  making progress on other futures
        tokio::task::spawn_blocking(move || {
            let metric_families = registry.gather();
            let mut buffer = Vec::new();
            let encoder = TextEncoder::new();

            encoder
                .encode(&metric_families, &mut buffer)
                .map_err(|e| ExpositionError::Encoding("unable to encode Prometheus metrics", Box::new(e)))
                .map(|_| buffer)
        })
        .await
        .map_err(|e| ExpositionError::Runtime("unable to gather Prometheus metrics", Box::new(e)))?
    }
}
