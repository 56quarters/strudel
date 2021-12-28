use clap::{crate_version, Parser};
use pitemp;
use rppal::gpio::{Gpio, IoPin, Mode, Pin};
use std::thread;
use std::time::Duration;

const PIN_NUM: u8 = 17;

const DHT_MAX_COUNT: usize = 256_000;
const DHT_PULSES: usize = 41;

struct TempReader {
    pin: IoPin,
}

impl TempReader {
    fn new(num: u8) -> Self {
        let controller = Gpio::new().unwrap();
        let pin = controller.get(num).unwrap();
        let io_pin = pin.into_io(Mode::Input);

        TempReader { pin: io_pin }
    }

    fn input(&mut self) {
        self.pin.set_mode(Mode::Input);
    }

    fn output(&mut self) {
        self.pin.set_mode(Mode::Output);
    }

    fn low(&mut self, dur: Duration) {
        self.pin.set_low();
        thread::sleep(dur);
    }

    fn high(&mut self, dur: Duration) {
        self.pin.set_high();
        thread::sleep(dur);
    }

    fn read_pulses(&mut self) -> [u32; DHT_PULSES * 2] {
        let mut pulse_counts: [u32; DHT_PULSES * 2] = [0; DHT_PULSES * 2];

        for i in (0..DHT_PULSES * 2).step_by(2) {
            while self.pin.is_low() {
                pulse_counts[i] += 1;
                if pulse_counts[i] >= DHT_MAX_COUNT as u32 {
                    panic!("timeout waiting for low pulse capture: {:?}", pulse_counts);
                }

            }

            while self.pin.is_high() {
                pulse_counts[i + 1] += 1;
                if pulse_counts[i + 1] >= DHT_MAX_COUNT as u32 {
                    panic!("timeout waiting for high pulse capture: {:?}", pulse_counts);
                }
            }
        }

        return pulse_counts;
    }
}

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

    let mut t = TempReader::new(PIN_NUM);
    t.output();
    t.high(Duration::from_millis(100));
    t.low(Duration::from_millis(20));
    t.high(Duration::from_micros(30));
    t.input();

    let pulses = t.read_pulses();
    println!("PULSES: {:?}", pulses);
}
