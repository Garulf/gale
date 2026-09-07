use std::time::{Duration, Instant};

pub const MAX_SAMPLE_AGE: Duration = Duration::from_secs(60);
pub const MAX_PLAUSIBLE_WATTS: f64 = 2000.0;

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

pub fn per_second(previous: Counter, now: Counter, modulus: Option<u64>) -> Option<f64> {
    let seconds = elapsed_secs(previous.at, now.at)?;
    let ticks = if now.value >= previous.value {
        now.value - previous.value
    } else {
        modulus?
            .checked_sub(previous.value)?
            .checked_add(now.value)?
    };
    Some(ticks as f64 / seconds)
}

pub fn plausible_watts(watts: f64) -> Option<f64> {
    (watts.is_finite() && (0.0..=MAX_PLAUSIBLE_WATTS).contains(&watts)).then_some(watts)
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
    fn a_wrapped_counter_adds_the_modulus_when_one_is_known() {
        let at = clock();
        let previous = Counter::new((1u64 << 32) - 10, at(0));
        let now = Counter::new(10, at(1));
        assert_eq!(per_second(previous, now, Some(1u64 << 32)), Some(20.0));
        assert_eq!(per_second(previous, now, None), None);
    }

    #[test]
    fn implausible_wattage_is_discarded() {
        assert_eq!(plausible_watts(150.0), Some(150.0));
        assert_eq!(plausible_watts(0.0), Some(0.0));
        assert_eq!(plausible_watts(MAX_PLAUSIBLE_WATTS + 0.5), None);
        assert_eq!(plausible_watts(-1.0), None);
        assert_eq!(plausible_watts(f64::NAN), None);
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
