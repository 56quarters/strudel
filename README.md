# Pi Temp

[![build status](https://circleci.com/gh/56quarters/pitemp.svg?style=shield)](https://circleci.com/gh/56quarters/pitemp)
[![docs.rs](https://docs.rs/pitemp/badge.svg)](https://docs.rs/pitemp/)
[![crates.io](https://img.shields.io/crates/v/donut.svg)](https://crates.io/crates/pitemp/)

Export DHT22 temperature and humidity sensor readings as Prometheus metrics.

## Features

`pitemp` reads temperature and humidity information from a [DHT22 sensor](https://learn.adafruit.com/dht)
and exports  the values as Prometheus metrics. It is best run on a Raspberry PI (3 or 4).

The following metrics are exported:

* `pitemp_temperature_celsius` - Degrees celsius measured by the sensor.
* `pitemp_relative_humidity` - Relative humidity (from 0 to 100) measured by the sensor.
* `pitemp_last_reading_timestamp` - UNIX timestamp of the last time the sensor was correctly read.
* `pitemp_collections_total` - Total number of attempts to read the sensor.
* `pitemp_errors_total` - Total errors by type while trying to read the sensor.

## Build

`pitemp` is a Rust program and must be built from source using a Rust toolchain. Since it's meant
to be run on a Raspberry PI, you will also likely need to cross-compile it. If you are on Ubuntu
Linux, you'll need the following packages installed for this.

```
apt-get install gcc-arm-linux-gnueabihf musl-tools
```

This will allow you to build for ARMv7 platforms and built completely static binaries (respectively).

Next, make sure you have a Rust toolchain for ARMv7, assuming you are using the `rustup` tool.

```
rustup target add armv7-unknown-linux-musleabihf
```

Next, you'll need to build `pitemp` itself for ARMv7.

```
cargo build --release --target armv7-unknown-linux-musleabihf
```

## Install

### GPIO Pin

In order to read the DHT22 sensor, it must be connected to one of the General Purpose IO pins (GPIO)
on your Raspberry PI (duh). The included Systemd unit file assumes that you have picked **GPIO pin 17**
for the data line of the sensor. If this is *not* the case, you'll have to modify the unit file. For
a list of available pins, see the [Raspberry PI documentation](https://www.raspberrypi.com/documentation/computers/os.html#gpio-and-the-40-pin-header).

### Run

In order to read and write the device `/dev/gpiomem`, `pitemp` must run as `root`. You can run
`pitemp` as a Systemd service using the [provided unit file](ext/pitemp.service). This unit file
assumes that you have copied the resulting `pitemp` binary to `/usr/local/bin/pitemp`.

```
sudo cp target/armv7-unknown-linux-musleabihf/release/pitemp /usr/local/bin/pitemp
sudo cp ext/pitemp.service /etc/systemd/system/pitemp.service
sudo systemctl daemon-reload
sudo systemctl enable pitemp.service
sudo systemctl start pitemp.serivce
```

### Prometheus

Prometheus metrics are exposed on port `9781` at `/metrics`. Once `pitemp`
is running, configure scrapes of it by your Prometheus server. Add the host running
`pitemp` as a target under the Prometheus `scrape_configs` section as described by
the example below.

**NOTE**: The DHT22 sensor can only be read every two seconds, at most. Thus the most
frequent Prometheus scrape interval that `pitemp` can support is `2s`. Something a bit
longer (like `10s` or `15s`) is recommended.

```yaml
# Sample config for Prometheus.

global:
  scrape_interval:     15s
  evaluation_interval: 15s
  external_labels:
      monitor: 'my_prom'

scrape_configs:
  - job_name: pitemp
    static_configs:
      - targets: ['example:9781']
```

## License

Pitemp is available under the terms of the [GPL, version 3](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you shall be licensed as above, without any
additional terms or conditions.
