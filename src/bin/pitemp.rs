use clap::{crate_version, Parser};
use pitemp;

/// Expose temperature and humidity from a DHT22 sensor as Prometheus metrics
///
/// Blah blah blah, longer description goes here.
#[derive(Debug, Parser)]
#[clap(name = "pitemp", version = crate_version!())]
struct PitempApplication {}

fn main() {
    let _opts = PitempApplication::parse();
    println!("Hello, world!");
    pitemp::do_the_thing();
}
