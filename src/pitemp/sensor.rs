use rppal::gpio::{Gpio, IoPin, Mode};
use std::error::Error;
use std::fmt;
use std::fmt::Formatter;
use std::thread;
use std::time::Duration;
use tracing::{event, Level};

const DHT_MAX_COUNT: u32 = 32_000;
const DHT_PULSES: usize = 41;
const DATA_SIZE: usize = 5;

///
///
///
#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct TemperatureCelsius(f64);

impl From<TemperatureFahrenheit> for TemperatureCelsius {
    fn from(f: TemperatureFahrenheit) -> Self {
        TemperatureCelsius((f.0 - 32.0) / 1.8)
    }
}

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

///
///
///
#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct TemperatureFahrenheit(f64);

impl From<TemperatureCelsius> for TemperatureFahrenheit {
    fn from(c: TemperatureCelsius) -> Self {
        TemperatureFahrenheit(c.0 * 1.8 + 32.0)
    }
}

impl From<TemperatureFahrenheit> for f64 {
    fn from(v: TemperatureFahrenheit) -> Self {
        v.0
    }
}

impl fmt::Display for TemperatureFahrenheit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}f", self.0)
    }
}

///
///
///
#[derive(Copy, Clone, Debug)]
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

///
///
///
#[derive(PartialEq, Eq, Debug, Hash, Clone, Copy)]
pub enum ErrorKind {
    Initialization,
    ReadTimeout,
    Checksum,
}

// TODO(56quarters): Do we even need this layer of indirection since this is an app, not a lib?
#[derive(Debug)]
enum ErrorRepr {
    CheckSum(u8, u8),
    KindMsg(ErrorKind, &'static str),
    KindMsgCause(ErrorKind, &'static str, Box<dyn Error + Send + Sync>),
}

///
///
///
#[derive(Debug)]
pub struct SensorError {
    repr: ErrorRepr,
}

impl SensorError {
    pub fn kind(&self) -> ErrorKind {
        match self.repr {
            ErrorRepr::CheckSum(_, _) => ErrorKind::Checksum,
            ErrorRepr::KindMsg(kind, _) => kind,
            ErrorRepr::KindMsgCause(kind, _, _) => kind,
        }
    }
}

impl fmt::Display for SensorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.repr {
            ErrorRepr::CheckSum(expected, got) => {
                write!(f, "checksum error: expected {}, got {}", expected, got)
            }
            ErrorRepr::KindMsg(_, msg) => msg.fmt(f),
            ErrorRepr::KindMsgCause(_, msg, ref e) => write!(f, "{}: {}", msg, e),
        }
    }
}

impl Error for SensorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.repr {
            ErrorRepr::KindMsgCause(_, _, ref e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<(u8, u8)> for SensorError {
    fn from((expected, got): (u8, u8)) -> Self {
        SensorError {
            repr: ErrorRepr::CheckSum(expected, got),
        }
    }
}

impl From<(ErrorKind, &'static str)> for SensorError {
    fn from((kind, msg): (ErrorKind, &'static str)) -> Self {
        SensorError {
            repr: ErrorRepr::KindMsg(kind, msg),
        }
    }
}

impl<E> From<(ErrorKind, &'static str, E)> for SensorError
where
    E: Error + Send + Sync + 'static,
{
    fn from((kind, msg, e): (ErrorKind, &'static str, E)) -> Self {
        SensorError {
            repr: ErrorRepr::KindMsgCause(kind, msg, Box::new(e)),
        }
    }
}

///
///
///
#[derive(Debug)]
struct Pulses {
    counts: [u32; DHT_PULSES * 2],
}

impl Pulses {
    ///
    ///
    ///
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
                    return Err(SensorError::from((
                        ErrorKind::ReadTimeout,
                        "timeout waiting for low pulse capture",
                    )));
                }
            }

            while pin.is_high() {
                counts[i + 1] += 1;
                if counts[i + 1] >= DHT_MAX_COUNT as u32 {
                    return Err(SensorError::from((
                        ErrorKind::ReadTimeout,
                        "timeout waiting for high pulse capture",
                    )));
                }
            }
        }

        event!(
            Level::TRACE,
            message = "reading low/high pulse counts",
            counts = ?counts,
        );

        Ok(Self { counts })
    }

    ///
    ///
    ///
    fn low(&self) -> impl Iterator<Item = &u32> {
        // Start from the 3rd element (first valid low count), emitting only low counts.
        // We're skipping the first low/high transition since the pin starts in the low
        // state when reading data and thus the first cycle count is always zero.
        self.counts.iter().skip(2).step_by(2)
    }

    ///
    ///
    ///
    fn high(&self) -> impl Iterator<Item = &u32> {
        // Start from the 4th element (first valid high count), emitting only high counts.
        // We're skipping the first low/high transition since the pin starts in the low
        // state when reading data and thus the first cycle count is always zero.
        self.counts.iter().skip(3).step_by(2)
    }
}

///
///
///
#[derive(Debug)]
struct Data {
    bytes: [u8; DATA_SIZE],
}

impl Data {
    ///
    ///
    ///
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

    ///
    ///
    ///
    fn pulse_threshold(pulses: &Pulses) -> u32 {
        let mut threshold = 0;
        let mut count = 0;

        for v in pulses.low() {
            threshold += v;
            count += 1;
        }

        threshold /= count;

        event!(
            Level::DEBUG,
            message = "computing threshold from low pulse average",
            threshold = threshold,
        );

        threshold
    }

    ///
    ///
    ///
    fn checksum(data: &[u8; DATA_SIZE]) -> Result<(), SensorError> {
        // From the DHT22 datasheet:
        // > If the data transmission is right, check-sum should be the last 8 bit of
        // > "8 bit integral RH data+8 bit decimal RH data+8 bit integral T data+8 bit
        // > decimal T data".
        let expected = data[4];
        let computed = ((data[0] as u16 + data[1] as u16 + data[2] as u16 + data[3] as u16) & 0xFF) as u8;

        event!(
            Level::DEBUG,
            message = "computing checksum for sensor data",
            computed = computed,
            expected = expected,
        );

        if computed != expected {
            Err(SensorError::from((expected, computed)))
        } else {
            Ok(())
        }
    }

    ///
    ///
    ///
    fn read(&self) -> (TemperatureCelsius, Humidity) {
        let hint = self.bytes[0] as f64;
        let hdec = self.bytes[1] as f64;
        let tint = self.bytes[2] as f64;
        let tdec = self.bytes[3] as f64;

        let humidity = Humidity(hint + (hdec / 10.0));
        let temperature = TemperatureCelsius(tint + (tdec / 10.0));

        event!(
            Level::DEBUG,
            message = "parsed sensor data",
            humidity_int = hint,
            humidity_dec = hdec,
            temperature_int = tint,
            temperature_dec = tdec,
            temperature = %temperature,
            humidity = %humidity
        );

        (temperature, humidity)
    }
}

///
///
///
#[derive(Debug)]
pub struct TemperatureReader {
    pin: IoPin,
}

impl TemperatureReader {
    ///
    ///
    ///
    pub fn new(bcm_gpio_pin: u8) -> Result<Self, SensorError> {
        let controller = Gpio::new()
            .map_err(|e| SensorError::from((ErrorKind::Initialization, "unable to create GPIO controller", e)))?;
        let pin = controller
            .get(bcm_gpio_pin)
            .map_err(|e| SensorError::from((ErrorKind::Initialization, "unable to acquire pin from controller", e)))?;
        let io_pin = pin.into_io(Mode::Input);

        Ok(Self { pin: io_pin })
    }

    ///
    ///
    ///
    fn prepare_for_read(&mut self) {
        // TODO(56quarters): Explain this, link to datasheet
        self.pin.set_mode(Mode::Output);
        self.pin.set_high();
        thread::sleep(Duration::from_millis(50));
        self.pin.set_low();
        thread::sleep(Duration::from_millis(30));
        self.pin.set_high();
        thread::sleep(Duration::from_micros(30));
        self.pin.set_mode(Mode::Input);
    }

    ///
    ///
    ///
    pub fn read(&mut self) -> Result<(TemperatureCelsius, Humidity), SensorError> {
        self.prepare_for_read();
        let pulses = Pulses::from_iopin(&self.pin)?;
        let parsed = Data::from_pulses(&pulses)?;
        Ok(parsed.read())
    }
}
