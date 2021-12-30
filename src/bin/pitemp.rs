use clap::{crate_version, Parser};
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use pitemp::http::{http_route, RequestContext};
use pitemp::metrics::{MetricsExposition, TemperatureMetrics};
use pitemp::sensor::TemperatureReader;
use std::net::SocketAddr;
use std::process;
use std::sync::Arc;
use std::time::Instant;
use tracing::{event, span, Instrument, Level};

// const PIN_NUM: u8 = 17;

const DEFAULT_LOG_LEVEL: Level = Level::INFO;
const DEFAULT_BIND_ADDR: ([u8; 4], u16) = ([127, 0, 0, 1], 3000);

/// Expose temperature and humidity from a DHT22 sensor as Prometheus metrics
///
/// Blah blah blah, longer description goes here.
#[derive(Debug, Parser)]
#[clap(name = "pitemp", version = crate_version!())]
struct PitempApplication {
    /// BCM GPIO pin number the DHT22 sensor data line is connected to
    #[clap(long)]
    bcm_pin: u8,

    /// Logging verbosity. Allowed values are 'trace', 'debug', 'info', 'warn', and 'error' (case insensitive)
    #[clap(long, default_value_t = DEFAULT_LOG_LEVEL)]
    log_level: Level,

    /// Address to bind to.
    #[clap(long, default_value_t = DEFAULT_BIND_ADDR.into())]
    bind: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opts = PitempApplication::parse();
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(opts.log_level)
            .finish(),
    )
    .expect("failed to set tracing subscriber");

    let startup = Instant::now();
    let reader = TemperatureReader::new(opts.bcm_pin).unwrap_or_else(|e| {
        event!(
            Level::ERROR,
            message = "failed to initialize sensor reader",
            bcm_pin = opts.bcm_pin,
            error = %e,
        );

        process::exit(1)
    });

    let reg = prometheus::default_registry().clone();
    reg.register(Box::new(TemperatureMetrics::new(reader)))
        .unwrap_or_else(|e| {
            event!(
                Level::ERROR,
                message = "failed to register sensor metric collector",
                error = %e,
            );

            process::exit(1)
        });

    let metrics = MetricsExposition::new(reg);
    let context = Arc::new(RequestContext::new(metrics));
    let service = make_service_fn(move |_| {
        let context = context.clone();

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                http_route(req, context.clone()).instrument(span!(Level::DEBUG, "pitemp_request"))
            }))
        }
    });
    let server = Server::try_bind(&opts.bind).unwrap_or_else(|e| {
        event!(
            Level::ERROR,
            message = "server failed to start",
            address = %opts.bind,
            error = %e,
        );

        process::exit(1);
    });

    event!(
        Level::INFO,
        message = "server started",
        address = %opts.bind,
        bcm_pin = opts.bcm_pin,
    );

    server
        .serve(service)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    event!(
        Level::INFO,
        message = "server shutdown",
        runtime_secs = %startup.elapsed().as_secs(),
    );

    Ok(())
}
