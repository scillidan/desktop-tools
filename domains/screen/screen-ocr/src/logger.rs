use std::sync::OnceLock;

static LOGGER: OnceLock<fn(&str)> = OnceLock::new();

pub fn set_logger(logger: fn(&str)) {
    let _ = LOGGER.set(logger);
}

pub fn log(message: &str) {
    if let Some(logger) = LOGGER.get() {
        logger(message);
    }
}
