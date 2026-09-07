use std::collections::HashMap;
use std::time::Instant;

use gale_hw::rate::{elapsed_secs, per_second, plausible_watts, Counter};
use gale_hw::{Backend, HwError, Id, Inventory, SensorInfo, SensorKind};

pub const PREFIX: &str = "cpu";
pub const MICROJOULES_PER_JOULE: f64 = 1_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub idle: u64,
    pub total: u64,
}

pub trait ProcStat: Send {
    fn read(&mut self) -> Result<CpuTimes, String>;
}

pub trait CpuFreq: Send {
    fn read_mhz(&mut self) -> Result<Vec<f64>, String>;
}

pub trait Rapl: Send {
    fn read_uj(&mut self) -> Result<u64, String>;
    fn energy_modulus_uj(&self) -> Option<u64>;
}

#[derive(Default)]
pub struct CpuSources {
    pub usage: Option<Box<dyn ProcStat>>,
    pub clock: Option<Box<dyn CpuFreq>>,
    pub power: Option<Box<dyn Rapl>>,
}

pub fn usage_percent(previous: (CpuTimes, Instant), now: (CpuTimes, Instant)) -> Option<f64> {
    elapsed_secs(previous.1, now.1)?;
    let total = now.0.total.checked_sub(previous.0.total)?;
    let idle = now.0.idle.checked_sub(previous.0.idle)?;
    if total == 0 || idle > total {
        return None;
    }
    Some((1.0 - idle as f64 / total as f64) * 100.0)
}

pub fn mean_mhz(cores: &[f64]) -> Option<f64> {
    let live: Vec<f64> = cores.iter().copied().filter(|mhz| *mhz > 0.0).collect();
    if live.is_empty() {
        return None;
    }
    Some(live.iter().sum::<f64>() / live.len() as f64)
}

pub struct CpuBackend {
    sources: CpuSources,
    sensors: Vec<SensorInfo>,
    previous_times: Option<(CpuTimes, Instant)>,
    previous_energy: Option<Counter>,
    clock: Box<dyn FnMut() -> Instant + Send>,
}

fn id(name: &str) -> Id {
    format!("{PREFIX}/{name}")
}

impl CpuBackend {
    pub fn new(sources: CpuSources) -> Self {
        Self {
            sources,
            sensors: Vec::new(),
            previous_times: None,
            previous_energy: None,
            clock: Box::new(Instant::now),
        }
    }

    pub fn with_clock(mut self, clock: Box<dyn FnMut() -> Instant + Send>) -> Self {
        self.clock = clock;
        self
    }

    fn has(&self, name: &str) -> bool {
        let wanted = id(name);
        self.sensors.iter().any(|sensor| sensor.id == wanted)
    }

    fn read_usage(&mut self) -> Option<f64> {
        let source = self.sources.usage.as_mut()?;
        let sample = match source.read() {
            Ok(times) => (times, (self.clock)()),
            Err(message) => {
                tracing::debug!(%message, "cpu usage read failed");
                return None;
            }
        };
        let percent = self
            .previous_times
            .and_then(|previous| usage_percent(previous, sample));
        self.previous_times = Some(sample);
        percent
    }

    fn read_clock(&mut self) -> Option<f64> {
        let source = self.sources.clock.as_mut()?;
        match source.read_mhz() {
            Ok(cores) => mean_mhz(&cores),
            Err(message) => {
                tracing::debug!(%message, "cpu clock read failed");
                None
            }
        }
    }

    fn read_power(&mut self) -> Option<f64> {
        let source = self.sources.power.as_mut()?;
        let sample = match source.read_uj() {
            Ok(microjoules) => Counter::new(microjoules, (self.clock)()),
            Err(message) => {
                tracing::debug!(%message, "cpu package power read failed");
                return None;
            }
        };
        let modulus = source.energy_modulus_uj();
        let watts = self
            .previous_energy
            .and_then(|previous| per_second(previous, sample, modulus))
            .map(|microjoules_per_second| microjoules_per_second / MICROJOULES_PER_JOULE)
            .and_then(plausible_watts);
        self.previous_energy = Some(sample);
        watts
    }
}

impl Backend for CpuBackend {
    fn name(&self) -> &str {
        PREFIX
    }

    fn enumerate(&mut self) -> Result<Inventory, HwError> {
        let mut sensors = Vec::new();
        if let Some(source) = self.sources.usage.as_mut() {
            match source.read() {
                Ok(_) => sensors.push(SensorInfo {
                    id: id("usage"),
                    label: "CPU Usage".to_string(),
                    kind: SensorKind::Percent,
                }),
                Err(message) => tracing::debug!(%message, "cpu usage unavailable"),
            }
        }
        if let Some(source) = self.sources.clock.as_mut() {
            match source.read_mhz() {
                Ok(cores) if mean_mhz(&cores).is_some() => sensors.push(SensorInfo {
                    id: id("clock"),
                    label: "CPU Clock".to_string(),
                    kind: SensorKind::Clock,
                }),
                Ok(_) => tracing::debug!("cpu clock reported no live cores"),
                Err(message) => tracing::debug!(%message, "cpu clock unavailable"),
            }
        }
        if let Some(source) = self.sources.power.as_mut() {
            match source.read_uj() {
                Ok(_) => sensors.push(SensorInfo {
                    id: id("power"),
                    label: "CPU Package Power".to_string(),
                    kind: SensorKind::Power,
                }),
                Err(message) => tracing::debug!(%message, "cpu package power unavailable"),
            }
        }
        self.sensors = sensors.clone();
        Ok(Inventory {
            sensors,
            controls: Vec::new(),
        })
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        let mut values = HashMap::new();
        if self.has("usage") {
            let usage = self.read_usage();
            values.insert(id("usage"), usage);
        }
        if self.has("clock") {
            let clock = self.read_clock();
            values.insert(id("clock"), clock);
        }
        if self.has("power") {
            let power = self.read_power();
            values.insert(id("power"), power);
        }
        values
    }

    fn set_duty(&mut self, id: &str, _pct: f64) -> Result<(), HwError> {
        Err(HwError::UnknownId(id.to_string()))
    }

    fn release(&mut self, id: &str) -> Result<(), HwError> {
        Err(HwError::UnknownId(id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::time::Duration;

    #[derive(Default)]
    struct FakeStat {
        samples: VecDeque<Result<CpuTimes, String>>,
    }

    impl ProcStat for FakeStat {
        fn read(&mut self) -> Result<CpuTimes, String> {
            self.samples
                .pop_front()
                .unwrap_or_else(|| Err("exhausted".to_string()))
        }
    }

    #[derive(Default)]
    struct FakeFreq {
        samples: VecDeque<Result<Vec<f64>, String>>,
    }

    impl CpuFreq for FakeFreq {
        fn read_mhz(&mut self) -> Result<Vec<f64>, String> {
            self.samples
                .pop_front()
                .unwrap_or_else(|| Err("exhausted".to_string()))
        }
    }

    #[derive(Default)]
    struct FakeRapl {
        samples: VecDeque<Result<u64, String>>,
        modulus: Option<u64>,
    }

    impl Rapl for FakeRapl {
        fn read_uj(&mut self) -> Result<u64, String> {
            self.samples
                .pop_front()
                .unwrap_or_else(|| Err("exhausted".to_string()))
        }
        fn energy_modulus_uj(&self) -> Option<u64> {
            self.modulus
        }
    }

    fn one_second_per_call() -> Box<dyn FnMut() -> Instant + Send> {
        let base = Instant::now();
        let mut seconds = 0;
        Box::new(move || {
            seconds += 1;
            base + Duration::from_secs(seconds)
        })
    }

    fn power_only_backend(samples: [Result<u64, String>; 3]) -> CpuBackend {
        CpuBackend::new(CpuSources {
            usage: None,
            clock: None,
            power: Some(Box::new(FakeRapl {
                samples: samples.into(),
                modulus: None,
            })),
        })
        .with_clock(one_second_per_call())
    }

    fn times(idle: u64, total: u64) -> Result<CpuTimes, String> {
        Ok(CpuTimes { idle, total })
    }

    fn at(offset_secs: u64) -> Instant {
        Instant::now() + Duration::from_secs(offset_secs)
    }

    #[test]
    fn usage_is_the_non_idle_share_of_the_jiffy_delta() {
        let previous = (
            CpuTimes {
                idle: 100,
                total: 400,
            },
            at(0),
        );
        let now = (
            CpuTimes {
                idle: 175,
                total: 700,
            },
            at(1),
        );
        assert_eq!(usage_percent(previous, now), Some(75.0));
    }

    #[test]
    fn usage_rejects_stale_gaps_flat_totals_and_counter_resets() {
        let previous = (
            CpuTimes {
                idle: 100,
                total: 400,
            },
            at(0),
        );
        assert_eq!(
            usage_percent(
                previous,
                (
                    CpuTimes {
                        idle: 175,
                        total: 700
                    },
                    at(61)
                )
            ),
            None
        );
        assert_eq!(
            usage_percent(
                previous,
                (
                    CpuTimes {
                        idle: 100,
                        total: 400
                    },
                    at(1)
                )
            ),
            None
        );
        assert_eq!(
            usage_percent(previous, (CpuTimes { idle: 0, total: 10 }, at(1))),
            None
        );
    }

    #[test]
    fn clock_is_the_mean_of_the_cores_that_report_a_frequency() {
        assert_eq!(mean_mhz(&[3000.0, 4000.0, 0.0]), Some(3500.0));
        assert_eq!(mean_mhz(&[]), None);
        assert_eq!(mean_mhz(&[0.0]), None);
    }

    fn backend_with_all_sources() -> CpuBackend {
        CpuBackend::new(CpuSources {
            usage: Some(Box::new(FakeStat {
                samples: [times(100, 400), times(100, 400), times(175, 700)].into(),
            })),
            clock: Some(Box::new(FakeFreq {
                samples: [Ok(vec![3000.0]), Ok(vec![3000.0, 4000.0])].into(),
            })),
            power: Some(Box::new(FakeRapl {
                samples: [Ok(0), Ok(1_000_000), Ok(3_000_000)].into(),
                modulus: Some(4_000_000),
            })),
        })
        .with_clock(one_second_per_call())
    }

    #[test]
    fn enumerate_lists_every_source_that_reads() {
        let mut backend = backend_with_all_sources();
        let inventory = backend.enumerate().unwrap();
        assert_eq!(
            inventory
                .sensors
                .iter()
                .map(|s| (s.id.as_str(), s.kind))
                .collect::<Vec<_>>(),
            vec![
                ("cpu/usage", SensorKind::Percent),
                ("cpu/clock", SensorKind::Clock),
                ("cpu/power", SensorKind::Power),
            ]
        );
        assert_eq!(inventory.sensors[2].label, "CPU Package Power");
        assert!(inventory.controls.is_empty());
    }

    #[test]
    fn a_source_that_cannot_be_read_is_left_out_of_the_inventory() {
        let mut backend = CpuBackend::new(CpuSources {
            usage: Some(Box::new(FakeStat::default())),
            clock: Some(Box::new(FakeFreq {
                samples: [Ok(vec![0.0])].into(),
            })),
            power: None,
        });
        assert!(backend.enumerate().unwrap().sensors.is_empty());
        assert!(backend.read_all().is_empty());
    }

    #[test]
    fn the_first_tick_has_no_usage_or_power_and_the_second_reports_rates() {
        let mut backend = backend_with_all_sources();
        backend.enumerate().unwrap();
        let first = backend.read_all();
        assert_eq!(first["cpu/usage"], None);
        assert_eq!(first["cpu/power"], None);
        assert_eq!(first["cpu/clock"], Some(3500.0));

        let second = backend.read_all();
        assert_eq!(second["cpu/usage"], Some(75.0));
        assert_eq!(second["cpu/power"], Some(1.0));
    }

    #[test]
    fn package_power_divides_the_microjoule_delta_by_the_elapsed_seconds() {
        let mut backend = power_only_backend([Ok(0), Ok(1_000_000), Ok(8_500_000)]);
        backend.enumerate().unwrap();
        assert_eq!(backend.read_all()["cpu/power"], None);
        assert_eq!(backend.read_all()["cpu/power"], Some(7.5));
    }

    #[test]
    fn an_implausible_wattage_after_a_suspend_is_discarded() {
        let mut backend = power_only_backend([Ok(0), Ok(0), Ok(3_000_000_000)]);
        backend.enumerate().unwrap();
        backend.read_all();
        assert_eq!(backend.read_all()["cpu/power"], None);
    }

    #[test]
    fn a_read_error_yields_none_without_dropping_the_sensor() {
        let mut backend = CpuBackend::new(CpuSources {
            usage: Some(Box::new(FakeStat {
                samples: [times(100, 400), Err("boom".to_string())].into(),
            })),
            clock: None,
            power: None,
        });
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["cpu/usage"], None);
        assert_eq!(values.len(), 1);
    }

    #[test]
    fn the_backend_exposes_no_controls() {
        let mut backend = backend_with_all_sources();
        assert!(matches!(
            backend.set_duty("cpu/usage", 50.0),
            Err(HwError::UnknownId(_))
        ));
        assert!(matches!(
            backend.release("cpu/usage"),
            Err(HwError::UnknownId(_))
        ));
        assert_eq!(backend.name(), "cpu");
    }
}
