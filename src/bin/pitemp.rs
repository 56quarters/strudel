use clap::{crate_version, Parser};
use pitemp::sensor::{TemperatureFahrenheit, TemperatureReader};
use tracing::Level;

// const PIN_NUM: u8 = 17;

/// Expose temperature and humidity from a DHT22 sensor as Prometheus metrics
///
/// Blah blah blah, longer description goes here.
#[derive(Debug, Parser)]
#[clap(name = "pitemp", version = crate_version!())]
struct PitempApplication {
    /// Logging verbosity. Allowed values are 'trace', 'debug', 'info', 'warn', and 'error' (case insensitive)
    #[clap(long, default_value_t = Level::INFO)]
    log_level: Level,

    /// BCM GPIO pin number the DHT22 sensor data line is connected to
    #[clap(long)]
    bcm_pin: u8,
}

fn main() {
    let opts = PitempApplication::parse();
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(opts.log_level)
            .finish(),
    )
    .expect("Failed to set tracing subscriber");

    let mut reader = TemperatureReader::new(opts.bcm_pin).unwrap();
    let (temp, humid) = reader.read().unwrap();

    println!(
        "TEMP C: {}, TEMP F: {}, HUMID: {}",
        temp,
        TemperatureFahrenheit::from(temp),
        humid
    );
}
