# Changelog

## [v0.7.0](https://github.com/56quarters/strudel/tree/0.7.0) - 2022-10-10

* Switch to the official Prometheus client and change sensor reads to happen independent
  of Prometheus scrapes. [#11](https://github.com/56quarters/strudel/pull/11)
* Minor dashboard improvements. [#12](https://github.com/56quarters/strudel/pull/12)
* Change default sensor read interval to `30s`. [#13](https://github.com/56quarters/strudel/pull/13)
* Dependency version updates. [#15](https://github.com/56quarters/strudel/pull/15)

## [v0.6.0](https://github.com/56quarters/strudel/tree/0.6.0) - 2022-05-24

* Switch to the Warp framework for HTTP routing. [#6](https://github.com/56quarters/strudel/pull/6)
* Fix parsing of sensor data to correctly read humidity. Thank you to `@tomasff` for
  reporting and debugging this issue. [#9](https://github.com/56quarters/strudel/pull/9)

## [v0.5.0](https://github.com/56quarters/strudel/tree/0.5.0) - 2022-01-15

* Gracefully shutdown on `SIGINT` or `SIGTERM`. [#4](https://github.com/56quarters/strudel/pull/4)
* Rename the metric `strudel_temperature_celsius` to `strudel_temperature_degrees`

## [v0.4.0](https://github.com/56quarters/strudel/tree/0.4.0) - 2022-01-10

* Restrict what `strudel` can do when running as a Systemd service.

## [v0.3.0](https://github.com/56quarters/strudel/tree/0.3.0) - 2022-01-03

* Rename the metric `strudel_last_reading_timestamp` to `strudel_last_read_timestamp`.
* Add `strudel_read_timing_seconds` to record how long sensor readings take.
* Remove unused dependencies.

## [v0.2.0](https://github.com/56quarters/strudel/tree/0.2.0) - 2022-01-02

* Documentation improvements.
* Module reorganization.

## [v0.1.0](https://github.com/56quarters/strudel/tree/0.1.0) - 2022-01-01

* Initial release.
