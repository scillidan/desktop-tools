//! Restart policy shared by the host. The host owns the process handles; this
//! module only answers "how long until the next attempt?".

use std::time::Duration;

/// Delays after unexpected exits: 1 s, 2 s, 5 s, then give up.
pub fn backoff(attempt: u32) -> Option<Duration> {
    match attempt {
        0 => Some(Duration::from_secs(1)),
        1 => Some(Duration::from_secs(2)),
        2 => Some(Duration::from_secs(5)),
        _ => None,
    }
}

/// Max restarts before a plugin is marked failed.
pub const MAX_RESTARTS: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_sequence_then_give_up() {
        assert_eq!(backoff(0), Some(Duration::from_secs(1)));
        assert_eq!(backoff(1), Some(Duration::from_secs(2)));
        assert_eq!(backoff(2), Some(Duration::from_secs(5)));
        assert_eq!(backoff(3), None);
        assert_eq!(backoff(99), None);
    }
}
