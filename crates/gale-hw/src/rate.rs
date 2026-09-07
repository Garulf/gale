use std::time::{Duration, Instant};

pub const MAX_SAMPLE_AGE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy)]
pub struct Counter {
    pub value: u64,
    pub at: Instant,
}

impl Counter {
    pub fn new(value: u64, at: Instant) -> Self {
        Self { value, at }
    }
}

pub fn elapsed_secs(previous: Instant, now: Instant) -> Option<f64> {
    let elapsed = now.checked_duration_since(previous)?;
    if elapsed.is_zero() || elapsed > MAX_SAMPLE_AGE {
        return None;
    }
    Some(elapsed.as_secs_f64())
}

pub fn per_second(previous: Counter, now: Counter, wrap_at: Option<u64>) -> Option<f64> {
    let seconds = elapsed_secs(previous.at, now.at)?;
    let ticks = if now.value >= previous.value {
        now.value - previous.value
    } else {
        wrap_at?
            .checked_sub(previous.value)?
            .checked_add(now.value)?
    };
    Some(ticks as f64 / seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> impl Fn(u64) -> Instant {
        let base = Instant::now();
        move |offset_secs| base + Duration::from_secs(offset_secs)
    }

    #[test]
    fn a_rising_counter_divides_the_delta_by_the_elapsed_seconds() {
        let at = clock();
        let previous = Counter::new(100, at(0));
        let now = Counter::new(400, at(2));
        assert_eq!(per_second(previous, now, None), Some(150.0));
    }

    #[test]
    fn a_wrapped_counter_adds_the_range_when_one_is_known() {
        let at = clock();
        let previous = Counter::new(u64::from(u32::MAX) - 9, at(0));
        let now = Counter::new(10, at(1));
        assert_eq!(
            per_second(previous, now, Some(u64::from(u32::MAX))),
            Some(19.0)
        );
        assert_eq!(per_second(previous, now, None), None);
    }

    #[test]
    fn samples_older_than_a_minute_and_zero_length_gaps_are_discarded() {
        let at = clock();
        assert_eq!(
            per_second(Counter::new(0, at(0)), Counter::new(100, at(61)), None),
            None
        );
        let same = at(0);
        assert_eq!(
            per_second(Counter::new(0, same), Counter::new(100, same), None),
            None
        );
    }

    #[test]
    fn a_backwards_clock_yields_nothing() {
        let at = clock();
        assert_eq!(
            per_second(Counter::new(0, at(5)), Counter::new(100, at(1)), None),
            None
        );
        assert_eq!(elapsed_secs(at(5), at(1)), None);
    }
}
