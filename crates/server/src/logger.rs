use log::LevelFilter;
use std::io::Write;

use chrono::Local;

pub(crate) fn setup_logger() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::new()
        .format(|buf, record| {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            writeln!(
                buf,
                "[{} {} {}:{}] {}",
                timestamp,
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .filter_level(LevelFilter::Info) // Default level
        .parse_env("RUST_LOG") // Override with env var if present
        .init();

    Ok(())
}
