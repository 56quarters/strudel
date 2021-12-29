use clap::{crate_version, Parser, values_t_or_exit};
use pitemp;
use rppal::gpio::{Gpio, IoPin, Mode};
use std::thread;
use std::time::Duration;

const PIN_NUM: u8 = 17;

const DHT_MAX_COUNT: usize = 128_000;
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
        // TODO(56quarters): Should we include the "setup" high/low switches here? Maybe put them in a setup() method?
        // TODO(56quarters): Should we enforce the 2 sec sample limit?
        let mut pulse_counts: [u32; DHT_PULSES * 2] = [0; DHT_PULSES * 2];

        for i in (0..DHT_PULSES * 2).step_by(2) {
            while self.pin.is_low() {
                pulse_counts[i] += 1;
                if pulse_counts[i] >= DHT_MAX_COUNT as u32 {
                    // TODO(56quarters): Real error handling since this might actually happen
                    panic!("timeout waiting for low pulse capture: {:?}", pulse_counts);
                }

                //thread::sleep(Duration::from_micros(1));
            }

            while self.pin.is_high() {
                pulse_counts[i + 1] += 1;
                if pulse_counts[i + 1] >= DHT_MAX_COUNT as u32 {
                    // TODO(56quarters): Real error handling since this might actually happen
                    panic!("timeout waiting for high pulse capture: {:?}", pulse_counts);
                }

                //thread::sleep(Duration::from_micros(1));
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
    t.high(Duration::from_millis(1000));
    t.low(Duration::from_millis(30));
    t.high(Duration::from_micros(30));
    t.input();

    let pulses = t.read_pulses();
    let mut total = 0;
    for i in (2..DHT_PULSES * 2).step_by(2) {
        total += pulses[i];
    }
    let avg = total / (DHT_PULSES as u32 - 1);

    println!("LOW AVG: {}", avg);
    println!("PULSES: {:?}", pulses);

    let mut data: [u8; 5] = [0; 5];
    for i in (3..DHT_PULSES * 2).step_by(2) {
        let data_index = (i - 3) / 16; // 40*2 / 16 == 5

        println!("i: {}, data index: {}, pulse: {}", i, data_index, pulses[i]);
        data[data_index] <<= 1; // shift current value left

        if pulses[i] >= avg {
            data[data_index] |= 1; // add 1, flipping right most bit
        }
    }

    for (i, v) in data.iter().enumerate() {
        println!("I: {}, Vi: {}, Vb: {:#8b}", i, v, v);
    }

    let hint = data[0] as f32;
    let hdec = data[1] as f32;
    let tint = data[2] as f32;
    let tdec = data[3] as f32;

    // From the DHT22 datasheet:
    // > If the data transmission is right, check-sum should be the last 8 bit of
    // > "8 bit integral RH data+8 bit decimal RH data+8 bit integral T data+8 bit
    // > decimal T data".
    let total1: u16 = data[0..4].iter().map(|v| *v as u16).sum();
    let res1 = (total1 & 0xFF) as u8;
    let total: u16 = data[0] as u16 + data[1] as u16 + data[2] as u16 + data[3] as u16;
    println!("TOTAL1: {}, TOTAL: {}", total1, total);
    let res = (total & 0xFF) as u8;
    println!("RES: {}, RES1: {}, CS: {}", res, res1, data[4]);

    let humid = hint + (hdec / 10.0);
    let temp = tint + (tdec / 10.0);

    println!(
        "TEMP C: {}, TEMP F: {}, HUMID: {}",
        temp,
        (temp * 1.8) + 32.0,
        humid
    );

    /*
    if data[4] == data[0] + data[1] + data[2] + data[3] {
        println!("VALID!");
    } else {
        println!("INVALID!");
    }
     */
}
