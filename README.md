# Strudel

![build status](https://github.com/56quarters/strudel/actions/workflows/rust.yml/badge.svg)
[![docs.rs](https://docs.rs/strudel/badge.svg)](https://docs.rs/strudel/)
[![crates.io](https://img.shields.io/crates/v/strudel.svg)](https://crates.io/crates/strudel/)

Export DHT22 temperature and humidity sensor readings as Prometheus metrics.

## Features

Strudel reads temperature and humidity information from a [DHT22 sensor](https://learn.adafruit.com/dht)
and exports  the values as Prometheus metrics. It is best run on a Raspberry PI (3 or 4).

The following metrics are exported:

* `strudel_temperature_degrees` - Degrees celsius measured by the sensor.
* `strudel_relative_humidity` - Relative humidity (from 0 to 100) measured by the sensor.
* `strudel_last_read_timestamp` - UNIX timestamp of the last time the sensor was correctly read.
* `strudel_collections_total` - Total number of attempts to read the sensor.
* `strudel_errors_total` - Total errors by type while trying to read the sensor.
* `strudel_read_timing_seconds` - Time taken to read the sensor, in seconds.

## Build

`strudel` is a Rust program and must be built from source using a [Rust toolchain](https://rustup.rs/)
. Since it's meant  to be run on a Raspberry PI, you will also likely need to cross-compile it. If you
are on Ubuntu GNU/Linux, you'll need the following packages installed for this.

```text
apt-get install gcc-arm-linux-gnueabihf musl-tools
```

This will allow you to build for ARMv7 platforms and build completely static binaries (respectively).

Next, make sure you have a Rust toolchain for ARMv7, assuming you are using the `rustup` tool.

```text
rustup target add armv7-unknown-linux-musleabihf
```

Next, you'll need to build `strudel` itself for ARMv7.

```text
cargo build --release --target armv7-unknown-linux-musleabihf
```

## Install

### GPIO Pin

In order to read the DHT22 sensor, it must be connected to one of the General Purpose IO pins (GPIO)
on your Raspberry PI (duh). The included Systemd unit file assumes that you have picked **GPIO pin 17**
for the data line of the sensor. If this is *not* the case, you'll have to modify the unit file. For
a list of available pins, see the [Raspberry PI documentation](https://www.raspberrypi.com/documentation/computers/os.html#gpio-and-the-40-pin-header).

### Run

In order to read and write the device `/dev/gpiomem`, `strudel` must run as `root`. You can run
`strudel` as a Systemd service using the [provided unit file](ext/strudel.service). This unit file
assumes that you have copied the resulting `strudel` binary to `/usr/local/bin/strudel`.

```text
sudo cp target/armv7-unknown-linux-musleabihf/release/strudel /usr/local/bin/strudel
sudo cp ext/strudel.service /etc/systemd/system/strudel.service
sudo systemctl daemon-reload
sudo systemctl enable strudel.service
sudo systemctl start strudel.serivce
```

### Prometheus

Prometheus metrics are exposed on port `9781` at `/metrics`. Once `strudel`
is running, configure scrapes of it by your Prometheus server. Add the host running
`strudel` as a target under the Prometheus `scrape_configs` section as described by
the example below.

**NOTE**: The DHT22 sensor can only be read every two seconds, at most. By default, the
sensor is read every `30s`, in the background (*not* in response to Prometheus scrapes).
Thus, scrapes by Prometheus more frequent than `30s` don't have any benefit unless the
refresh interval for `strudel` is adjusted as well.

```yaml
# Sample config for Prometheus.

global:
  scrape_interval:     1m
  evaluation_interval: 1m
  external_labels:
      monitor: 'my_prom'

scrape_configs:
  - job_name: strudel
    static_configs:
      - targets: ['example:9781']
```

## References

Some helpful documentation, articles, etc. used to create Strudel

* [Datasheet](https://cdn-shop.adafruit.com/datasheets/Digital+humidity+and+temperature+sensor+AM2302.pdf)
* [DHT11 Driver in Rust](https://www.speblog.org/blog/dht11-temperature-humidity-rust-driver-raspberry-pi/)
* [RPPal crate](https://crates.io/crates/rppal)

## License

Strudel is available under the terms of the [GPL, version 3](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you shall be licensed as above, without any
additional terms or conditions.
