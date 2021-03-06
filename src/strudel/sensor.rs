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

use rppal::gpio::{Gpio, IoPin, Mode};
use std::error::Error;
use std::fmt;
use std::fmt::Formatter;
use std::thread;
use std::time::Duration;

const DHT_MAX_COUNT: u32 = 32_000;
const DHT_PULSES: usize = 41;
const DATA_SIZE: usize = 5;

/// Temperature, in degrees celsius
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(transparent)]
pub struct TemperatureCelsius(f64);

impl From<TemperatureCelsius> for f64 {
    fn from(v: TemperatureCelsius) -> Self {
        v.0
    }
}

impl fmt::Display for TemperatureCelsius {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}c", self.0)
    }
}

/// Relative humidity (from 0 to 100)
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(transparent)]
pub struct Humidity(f64);

impl From<Humidity> for f64 {
    fn from(v: Humidity) -> Self {
        v.0
    }
}

impl fmt::Display for Humidity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

/// Potential kinds of errors that can be encountered reading from the DHT sensor
#[derive(PartialEq, Eq, Debug, Hash, Clone, Copy)]
pub enum SensorErrorKind {
    Initialization,
    ReadTimeout,
    Checksum,
}

impl SensorErrorKind {
    pub fn as_label(&self) -> &'static str {
        match self {
            SensorErrorKind::Initialization => "initialization",
            SensorErrorKind::ReadTimeout => "timeout",
            SensorErrorKind::Checksum => "checksum",
        }
    }
}

/// Error initializing or reading the DHT22 sensor via a GPIO pin
#[derive(Debug)]
pub enum SensorError {
    CheckSum(u8, u8),
    KindMsg(SensorErrorKind, &'static str),
    KindMsgCause(SensorErrorKind, &'static str, Box<dyn Error + Send + Sync>),
}

impl SensorError {
    pub fn kind(&self) -> SensorErrorKind {
        match self {
            SensorError::CheckSum(_, _) => SensorErrorKind::Checksum,
            SensorError::KindMsg(kind, _) => *kind,
            SensorError::KindMsgCause(kind, _, _) => *kind,
        }
    }
}

impl fmt::Display for SensorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SensorError::CheckSum(expected, got) => {
                write!(f, "checksum error: expected {}, got {}", expected, got)
            }
            SensorError::KindMsg(_, msg) => msg.fmt(f),
            SensorError::KindMsgCause(_, msg, ref e) => write!(f, "{}: {}", msg, e),
        }
    }
}

impl Error for SensorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SensorError::KindMsgCause(_, _, ref e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

/// Cycle counts of how long the sensor data pin spent low and high states.
///
/// There are 40 low/high transitions we count cycles for. These counts are
/// used to read 40 bits of information from the sensor.
#[derive(Debug)]
struct Pulses {
    // We store counts for 41 transitions but don't use the first low/high transition
    counts: [u32; DHT_PULSES * 2],
}

impl Pulses {
    /// Count the number of cycles the given pin spends in the low and high states for
    /// 40 low/high transitions.
    ///
    /// An error will be returned if the pin didn't transition in time. The read will have
    /// to be retried in this case.
    ///
    /// NOTE: This method assumes the pin as already been prepared for reading by sending
    /// and initial high-low-high transition with timings corresponding to the DHT22
    /// datasheet.
    fn from_iopin(pin: &IoPin) -> Result<Self, SensorError> {
        // Create an array with 2x the number of pulses we're going to measure so that we can
        // store the number of cycles the pin spent high and low for each pulse.
        let mut counts: [u32; DHT_PULSES * 2] = [0; DHT_PULSES * 2];

        // Store counts for both high and low states of the pin in the same array. We advance
        // by two entries each iteration of the loop but use (i + 1) to access the odd entries.
        //
        // We only store up to DHT_MAX_COUNT which is a much much higher number of cycles than
        // we expect to get in practice (normal number of cycles at high or low is 50 - 200).
        // This is done to enforce a timeout while waiting for the pin to switch between low
        // and high states. In this case, the read will have to be retried.
        for i in (0..counts.len()).step_by(2) {
            while pin.is_low() {
                counts[i] += 1;
                if counts[i] >= DHT_MAX_COUNT as u32 {
                    return Err(SensorError::KindMsg(
                        SensorErrorKind::ReadTimeout,
                        "timeout waiting for low pulse capture",
                    ));
                }
            }

            while pin.is_high() {
                counts[i + 1] += 1;
                if counts[i + 1] >= DHT_MAX_COUNT as u32 {
                    return Err(SensorError::KindMsg(
                        SensorErrorKind::ReadTimeout,
                        "timeout waiting for high pulse capture",
                    ));
                }
            }
        }

        tracing::trace!(message = "reading low/high pulse counts", counts = ?counts);
        Ok(Self { counts })
    }

    /// Return an iterator over 40 cycle counts for the pin in the low state.
    fn low(&self) -> impl Iterator<Item = &u32> {
        // Start from the 3rd element (first valid low count), emitting only low counts.
        // We're skipping the first low/high transition since the pin starts in the low
        // state when reading data and thus the first cycle count is always zero.
        self.counts.iter().skip(2).step_by(2)
    }

    /// Return an iterator over 40 cycle counts for the pin in the high state.
    fn high(&self) -> impl Iterator<Item = &u32> {
        // Start from the 4th element (first valid high count), emitting only high counts.
        // We're skipping the first low/high transition since the pin starts in the low
        // state when reading data and thus the first cycle count is always zero.
        self.counts.iter().skip(3).step_by(2)
    }
}

/// Sensor data parsed from low/high cycle counts.
#[derive(Debug)]
struct Reading {
    bytes: [u8; DATA_SIZE],
}

impl Reading {
    /// Parse sensor data from the provided low/high pulse counts.
    ///
    /// An error will be returned if the checksum included in the data indicates the data
    /// is corrupt.
    fn from_pulses(pulses: &Pulses) -> Result<Self, SensorError> {
        let mut bytes: [u8; DATA_SIZE] = [0; DATA_SIZE];

        // Find the average low pin cycle count so that we can determine if each high
        // pin cycle count is meant to be a 0 bit (lower than the threshold) or a 1 bit
        // (higher than the threshold).
        let threshold = Self::pulse_threshold(pulses);

        for (i, &v) in pulses.high().enumerate() {
            // There are 40 low/high transition cycle counts and hence 40 bits of data
            // that we need to parse. Divide by eight to figure out which byte this bit
            // will end up in and shift the current value left (we only operate on the
            // LSB each iteration).
            let index = i / 8;
            bytes[index] <<= 1;

            if v >= threshold {
                bytes[index] |= 1;
            }
        }

        // Byte five is a checksum of the first four bytes, return an error if it indicates
        // the data we've read is corrupt somehow.
        Self::checksum(&bytes)?;
        Ok(Self { bytes })
    }

    /// Determine the threshold for high cycle counts to be treated as 0 or 1 based
    /// on the average number of cycles the pin spends at low voltage.
    fn pulse_threshold(pulses: &Pulses) -> u32 {
        let mut threshold = 0;
        let mut count = 0;

        for v in pulses.low() {
            threshold += v;
            count += 1;
        }

        threshold /= count;

        tracing::debug!(
            message = "computing threshold from low pulse average",
            threshold = threshold
        );
        threshold
    }

    /// Return an error if the checksum (byte 5) indicates the data read is corrupt.
    fn checksum(data: &[u8; DATA_SIZE]) -> Result<(), SensorError> {
        // From the DHT22 datasheet:
        // > If the data transmission is right, check-sum should be the last 8 bit of
        // > "8 bit integral RH data+8 bit decimal RH data+8 bit integral T data+8 bit
        // > decimal T data".
        let expected = data[4];
        let computed = ((data[0] as u16 + data[1] as u16 + data[2] as u16 + data[3] as u16) & 0xFF) as u8;

        tracing::debug!(
            message = "computing checksum for sensor data",
            computed = computed,
            expected = expected
        );
        if computed != expected {
            Err(SensorError::CheckSum(expected, computed))
        } else {
            Ok(())
        }
    }

    /// Parse data bytes into temperature celsius and relative humidity.
    fn parse(&self) -> (TemperatureCelsius, Humidity) {
        // See https://cdn-shop.adafruit.com/datasheets/Digital+humidity+and+temperature+sensor+AM2302.pdf
        // first two bytes are humidity as a u16 * 10
        let h = (self.bytes[0] as u16) * 256 /* shift left 8 bits */ + self.bytes[1] as u16;
        // second two bytes are temperature as a u16 * 10 with the highest bit indicating sign
        let t = ((self.bytes[2] & 0b0111_1111) as u16) * 256 /* shift left 8 bits */ + self.bytes[3] as u16;

        let hdec = h as f64 / 10.0;
        let mut tdec = t as f64 / 10.0;
        // highest bit of the temperature is `1` to indicate a negative value
        if self.bytes[2] & 0b1000_0000 > 0 {
            tdec = -tdec;
        }

        let humidity = Humidity(hdec);
        let temperature = TemperatureCelsius(tdec);

        tracing::debug!(
            message = "parsed sensor data",
            raw_temperature = t,
            raw_humidity = h,
            temperature = %temperature,
            humidity = %humidity
        );

        (temperature, humidity)
    }
}

/// Read temperature in degrees celsius and relative humidity from a DHT22 sensor
#[derive(Debug)]
pub struct TemperatureReader {
    pin: IoPin,
}

impl TemperatureReader {
    /// Create a new reader based on the BCM GPIO pin number of the data wire of
    /// the DHT22 sensor.
    ///
    /// Note that the BCM GPIO pin number is NOT the same as the physical pin number.
    /// See [pinout] for more information.
    ///
    /// [pinout]: https://www.raspberrypi.com/documentation/computers/os.html#gpio-and-the-40-pin-header
    pub fn new(bcm_gpio_pin: u8) -> Result<Self, SensorError> {
        let controller = Gpio::new().map_err(|e| {
            SensorError::KindMsgCause(
                SensorErrorKind::Initialization,
                "unable to create GPIO controller",
                Box::new(e),
            )
        })?;
        let pin = controller.get(bcm_gpio_pin).map_err(|e| {
            SensorError::KindMsgCause(
                SensorErrorKind::Initialization,
                "unable to acquire pin from controller",
                Box::new(e),
            )
        })?;
        let io_pin = pin.into_io(Mode::Input);

        Ok(Self { pin: io_pin })
    }

    /// Send a high-low-high signal to indicate the sensor should perform a read
    fn prepare_for_read(&mut self) {
        // https://cdn-shop.adafruit.com/datasheets/Digital+humidity+and+temperature+sensor+AM2302.pdf
        // Host needs to set the sensor:
        // * high to start the read process, waking the sensor up from low-power mode
        // * low for at least 1ms to ensure the sensor detected the start of this process
        // * high for 20-40us to then wait for the sensor's response
        self.pin.set_mode(Mode::Output);
        self.pin.set_high();
        thread::sleep(Duration::from_millis(50));
        self.pin.set_low();
        thread::sleep(Duration::from_millis(30));
        self.pin.set_high();
        thread::sleep(Duration::from_micros(30));
        self.pin.set_mode(Mode::Input);
    }

    /// Read temperature and humidity from the sensor or return an error if the
    /// read failed with details about what caused the read to fail.
    ///
    /// Note the DHT22 sensor should only be read every two seconds at max. This shouldn't
    /// be an issue in practice since Prometheus scrape intervals are usually at least
    /// 10 seconds.
    pub fn read(&mut self) -> Result<(TemperatureCelsius, Humidity), SensorError> {
        self.prepare_for_read();
        let pulses = Pulses::from_iopin(&self.pin)?;
        let reading = Reading::from_pulses(&pulses)?;
        Ok(reading.parse())
    }
}

#[cfg(test)]
mod test {
    use super::{Humidity, Pulses, Reading, SensorError, TemperatureCelsius, DATA_SIZE, DHT_PULSES};

    fn new_counts() -> [u32; DHT_PULSES * 2] {
        #[rustfmt::skip]
        let counts = [
            0, 761,
            561, 307,
            559, 295,
            598, 307,
            589, 307,
            591, 307,
            598, 307,
            591, 866,
            592, 854,
            570, 748,
            598, 860,
            598, 859,
            596, 307,
            590, 307,
            598, 859,
            599, 859,
            598, 847,
            748, 300,
            593, 296,
            589, 307,
            598, 300,
            598, 307,
            591, 307,
            598, 307,
            590, 847,
            748, 307,
            592, 306,
            599, 299,
            598, 307,
            591, 307,
            598, 307,
            591, 307,
            591, 855,
            708, 867,
            590, 866,
            591, 867,
            591, 307,
            598, 859,
            598, 859,
            598, 300,
            598, 295,
        ];

        counts
    }

    #[test]
    fn test_reading_from_pulses() {
        let pulses = Pulses { counts: new_counts() };
        let res = Reading::from_pulses(&pulses);

        assert!(res.is_ok(), "unexpected error result: {:?}", res);

        let reading = res.unwrap();
        let (t, h) = reading.parse();

        assert_eq!(TemperatureCelsius(25.7), t);
        assert_eq!(Humidity(99.9), h);
    }

    #[test]
    fn test_reading_threshold() {
        let pulses = Pulses { counts: new_counts() };
        let threshold = Reading::pulse_threshold(&pulses);

        assert_eq!(602, threshold);
    }

    #[test]
    fn test_reading_checksum_valid() {
        let mut bytes = [0; DATA_SIZE];
        bytes[0] = 0b0000_0010; // humidity 1
        bytes[1] = 0b1000_1100; // humidity 2
        bytes[2] = 0b0000_0001; // temperature 1
        bytes[3] = 0b0101_1111; // temperature 2
        bytes[4] = 0b1110_1110; // checksum

        let res = Reading::checksum(&bytes);
        assert!(res.is_ok())
    }

    #[test]
    fn test_reading_checksum_invalid() {
        let mut bytes = [0; 5];
        bytes[0] = 0b0000_0010; // humidity 1
        bytes[1] = 0b1000_1100; // humidity 2
        bytes[2] = 0b0000_0001; // temperature 1
        bytes[3] = 0b0101_1111; // temperature 2
        bytes[4] = 0b0000_0000; // checksum

        let res = Reading::checksum(&bytes);
        assert!(res.is_err());

        match res.unwrap_err() {
            SensorError::CheckSum(expected, got) => {
                assert_eq!(0b0000_0000, expected); // `expected` is what is part of the data
                assert_eq!(0b1110_1110, got); // `got` is what was computed based on the data
            }
            SensorError::KindMsg(kind, msg) => {
                panic!("Unexpected error. kind: {:?}, message: {}", kind, msg);
            }
            SensorError::KindMsgCause(kind, msg, cause) => {
                panic!("Unexpected error. kind: {:?}, message: {}, cause: {}", kind, msg, cause);
            }
        }
    }

    #[test]
    fn test_reading_parse_positive_temp() {
        // Example data, from the datasheet: https://cdn-shop.adafruit.com/datasheets/Digital+humidity+and+temperature+sensor+AM2302.pdf
        let mut bytes = [0; 5];
        bytes[0] = 0b0000_0010; // humidity 1
        bytes[1] = 0b1000_1100; // humidity 2
        bytes[2] = 0b0000_0001; // temperature 1
        bytes[3] = 0b0101_1111; // temperature 2
        bytes[4] = 0b0000_0000; // checksum, ignored here

        let reading = Reading { bytes };
        let (t, h) = reading.parse();

        assert_eq!(TemperatureCelsius(35.1), t);
        assert_eq!(Humidity(65.2), h);
    }

    #[test]
    fn test_reading_parse_negative_temp() {
        // Example data, from the datasheet: https://cdn-shop.adafruit.com/datasheets/Digital+humidity+and+temperature+sensor+AM2302.pdf
        let mut bytes = [0; 5];
        bytes[0] = 0b0000_0010; // humidity 1
        bytes[1] = 0b1000_1100; // humidity 2
        bytes[2] = 0b1000_0000; // temperature 1
        bytes[3] = 0b0110_0101; // temperature 2
        bytes[4] = 0b0000_0000; // checksum, ignored here

        let reading = Reading { bytes };
        let (t, h) = reading.parse();

        assert_eq!(TemperatureCelsius(-10.1), t);
        assert_eq!(Humidity(65.2), h);
    }
}
