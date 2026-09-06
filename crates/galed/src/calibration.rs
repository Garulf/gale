use gale_hw::{Inventory, SensorKind};
use serde::Serialize;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

pub const STEP_PCT: f64 = 5.0;
pub const PROBE_PCT: f64 = 50.0;
pub const MARGIN_PCT: f64 = 2.0;
pub const SETTLE_SECS: u64 = 4;
const REST_CONFIRMATIONS: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CalibrationResult {
    pub min_duty: f64,
    pub start_duty: f64,
    pub stop_duty: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Probe,
    Down,
    Rest,
    Up,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    SetDuty(f64),
    Finished(CalibrationResult),
    Failed(String),
}

pub struct Sweep {
    phase: Phase,
    duty: f64,
    last_spinning: Option<f64>,
    stall: Option<f64>,
    rest_checks: u8,
}

fn spinning(rpm: Option<f64>) -> bool {
    matches!(rpm, Some(value) if value > 0.0)
}

fn with_margin(duty: f64) -> f64 {
    (duty + MARGIN_PCT).clamp(0.0, 100.0)
}

impl Default for Sweep {
    fn default() -> Self {
        Self::new()
    }
}

impl Sweep {
    pub fn new() -> Self {
        Self {
            phase: Phase::Probe,
            duty: PROBE_PCT,
            last_spinning: None,
            stall: None,
            rest_checks: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn duty(&self) -> f64 {
        self.duty
    }

    pub fn first(&self) -> Step {
        Step::SetDuty(self.duty)
    }

    pub fn observe(&mut self, rpm: Option<f64>) -> Step {
        match self.phase {
            Phase::Probe => {
                if !spinning(rpm) {
                    return Step::Failed(format!("no rpm reading at {PROBE_PCT} %"));
                }
                self.last_spinning = Some(self.duty);
                self.phase = Phase::Down;
                self.duty -= STEP_PCT;
                Step::SetDuty(self.duty)
            }
            Phase::Down => {
                if spinning(rpm) {
                    self.last_spinning = Some(self.duty);
                    if self.duty <= 0.0 {
                        return Step::Finished(CalibrationResult {
                            min_duty: 0.0,
                            start_duty: 0.0,
                            stop_duty: 0.0,
                        });
                    }
                    self.duty = (self.duty - STEP_PCT).max(0.0);
                    return Step::SetDuty(self.duty);
                }
                self.stall = Some(self.duty);
                self.phase = Phase::Rest;
                self.duty = 0.0;
                Step::SetDuty(0.0)
            }
            Phase::Rest => {
                if spinning(rpm) {
                    self.rest_checks += 1;
                    if self.rest_checks >= REST_CONFIRMATIONS {
                        return Step::Failed("fan keeps spinning at 0 %".into());
                    }
                    return Step::SetDuty(0.0);
                }
                self.phase = Phase::Up;
                self.duty = STEP_PCT;
                Step::SetDuty(self.duty)
            }
            Phase::Up => {
                if spinning(rpm) {
                    let min_duty = with_margin(self.last_spinning.unwrap_or(self.duty));
                    return Step::Finished(CalibrationResult {
                        min_duty,
                        start_duty: with_margin(self.duty).max(min_duty),
                        stop_duty: with_margin(self.stall.unwrap_or(0.0)),
                    });
                }
                if self.duty >= 100.0 {
                    return Step::Failed("fan did not start by 100 %".into());
                }
                self.duty = (self.duty + STEP_PCT).min(100.0);
                Step::SetDuty(self.duty)
            }
        }
    }
}

fn channel_of(id: &str) -> Option<&str> {
    let leaf = id.rsplit('/').next()?;
    let digits = leaf.trim_end_matches(|c: char| !c.is_ascii_digit());
    let start = digits
        .rfind(|c: char| !c.is_ascii_digit())
        .map(|i| i + 1)
        .unwrap_or(0);
    let channel = &digits[start..];
    (!channel.is_empty()).then_some(channel)
}

fn device_of(id: &str) -> &str {
    id.rsplit_once('/').map(|(device, _)| device).unwrap_or("")
}

pub fn tach_for(control: &str, inventory: &Inventory) -> Option<String> {
    let same_device: Vec<&str> = inventory
        .sensors
        .iter()
        .filter(|sensor| {
            sensor.kind == SensorKind::Rpm && device_of(&sensor.id) == device_of(control)
        })
        .map(|sensor| sensor.id.as_str())
        .collect();
    if same_device.contains(&control) {
        return Some(control.to_string());
    }
    let channel = channel_of(control)?;
    same_device
        .into_iter()
        .find(|sensor| channel_of(sensor) == Some(channel))
        .map(str::to_string)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Progress {
    Idle,
    Running {
        control: String,
        tach: String,
        phase: Phase,
        duty: f64,
        rpm: Option<f64>,
    },
    Done {
        control: String,
        result: CalibrationResult,
    },
    Failed {
        control: String,
        error: String,
    },
}

pub trait Actuator: Send + Sync {
    fn set_duty(&self, duty: f64) -> impl Future<Output = Result<(), String>> + Send;
    fn settle(&self) -> impl Future<Output = ()> + Send;
    fn rpm(&self) -> impl Future<Output = Option<f64>> + Send;
    fn release(&self) -> impl Future<Output = ()> + Send;
    fn persist(&self, result: CalibrationResult)
        -> impl Future<Output = Result<(), String>> + Send;
}

pub async fn run<A: Actuator>(
    actuator: &A,
    control: &str,
    tach: &str,
    progress: &watch::Sender<Progress>,
    cancel: &AtomicBool,
) -> Progress {
    let mut sweep = Sweep::new();
    let mut step = sweep.first();
    let outcome = loop {
        match step {
            Step::SetDuty(duty) => {
                if cancel.load(Ordering::SeqCst) {
                    break Progress::Failed {
                        control: control.to_string(),
                        error: "cancelled".into(),
                    };
                }
                if let Err(error) = actuator.set_duty(duty).await {
                    break Progress::Failed {
                        control: control.to_string(),
                        error,
                    };
                }
                progress.send_replace(Progress::Running {
                    control: control.to_string(),
                    tach: tach.to_string(),
                    phase: sweep.phase(),
                    duty,
                    rpm: None,
                });
                actuator.settle().await;
                let rpm = actuator.rpm().await;
                progress.send_replace(Progress::Running {
                    control: control.to_string(),
                    tach: tach.to_string(),
                    phase: sweep.phase(),
                    duty,
                    rpm,
                });
                step = sweep.observe(rpm);
            }
            Step::Finished(result) => {
                break match actuator.persist(result).await {
                    Ok(()) => Progress::Done {
                        control: control.to_string(),
                        result,
                    },
                    Err(error) => Progress::Failed {
                        control: control.to_string(),
                        error,
                    },
                };
            }
            Step::Failed(error) => {
                break Progress::Failed {
                    control: control.to_string(),
                    error,
                };
            }
        }
    };
    actuator.release().await;
    progress.send_replace(outcome.clone());
    outcome
}

pub struct Calibrator {
    progress: watch::Sender<Progress>,
    active: Mutex<Option<Arc<AtomicBool>>>,
}

impl Default for Calibrator {
    fn default() -> Self {
        Self::new()
    }
}

impl Calibrator {
    pub fn new() -> Self {
        let (progress, _) = watch::channel(Progress::Idle);
        Self {
            progress,
            active: Mutex::new(None),
        }
    }

    pub fn progress(&self) -> Progress {
        self.progress.borrow().clone()
    }

    pub fn is_running(&self) -> bool {
        matches!(self.progress(), Progress::Running { .. })
    }

    pub fn begin(&self) -> Result<Arc<AtomicBool>, String> {
        let mut active = self.active.lock().unwrap();
        if self.is_running() {
            return Err("a calibration is already running".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *active = Some(cancel.clone());
        Ok(cancel)
    }

    pub fn sender(&self) -> &watch::Sender<Progress> {
        &self.progress
    }

    pub fn cancel(&self) -> bool {
        match self.active.lock().unwrap().as_ref() {
            Some(flag) if self.is_running() => {
                flag.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_hw::SensorInfo;
    use std::sync::atomic::AtomicUsize;

    fn drive(sweep: &mut Sweep, rpm_for: impl Fn(Phase, f64) -> Option<f64>) -> (Step, Vec<f64>) {
        let mut duties = Vec::new();
        let mut step = sweep.first();
        for _ in 0..200 {
            match step {
                Step::SetDuty(duty) => {
                    duties.push(duty);
                    step = sweep.observe(rpm_for(sweep.phase(), duty));
                }
                other => return (other, duties),
            }
        }
        panic!("sweep did not terminate");
    }

    fn typical_fan(phase: Phase, duty: f64) -> Option<f64> {
        match phase {
            Phase::Probe | Phase::Down => (duty >= 20.0).then_some(600.0),
            Phase::Rest => None,
            Phase::Up => (duty >= 30.0).then_some(500.0),
        }
    }

    #[test]
    fn sweep_finds_min_stop_and_start_with_margins() {
        let mut sweep = Sweep::new();
        let (outcome, duties) = drive(&mut sweep, typical_fan);
        assert_eq!(
            outcome,
            Step::Finished(CalibrationResult {
                min_duty: 22.0,
                start_duty: 32.0,
                stop_duty: 17.0,
            })
        );
        assert_eq!(duties[0], 50.0);
        assert!(duties.contains(&15.0), "sweep must reach the stall step");
        assert!(duties.contains(&0.0), "sweep must rest at 0");
        assert_eq!(*duties.last().unwrap(), 30.0);
    }

    #[test]
    fn sweep_fails_when_the_tach_is_silent_at_the_probe_duty() {
        let mut sweep = Sweep::new();
        let (outcome, _) = drive(&mut sweep, |_, _| None);
        assert_eq!(outcome, Step::Failed("no rpm reading at 50 %".into()));
    }

    #[test]
    fn sweep_reports_zero_limits_for_a_fan_that_never_stops() {
        let mut sweep = Sweep::new();
        let (outcome, _) = drive(&mut sweep, |_, _| Some(300.0));
        assert_eq!(
            outcome,
            Step::Finished(CalibrationResult {
                min_duty: 0.0,
                start_duty: 0.0,
                stop_duty: 0.0,
            })
        );
    }

    #[test]
    fn sweep_fails_when_a_fan_stalls_but_never_restarts() {
        let mut sweep = Sweep::new();
        let (outcome, duties) = drive(&mut sweep, |phase, duty| match phase {
            Phase::Probe | Phase::Down => (duty >= 40.0).then_some(500.0),
            _ => None,
        });
        assert_eq!(outcome, Step::Failed("fan did not start by 100 %".into()));
        assert_eq!(*duties.last().unwrap(), 100.0);
    }

    #[test]
    fn sweep_waits_out_a_coasting_fan_before_the_upward_pass() {
        let mut sweep = Sweep::new();
        let rest_seen = AtomicUsize::new(0);
        let (outcome, _) = drive(&mut sweep, |phase, duty| match phase {
            Phase::Probe | Phase::Down => (duty >= 20.0).then_some(600.0),
            Phase::Rest => {
                let n = rest_seen.fetch_add(1, Ordering::SeqCst);
                (n == 0).then_some(120.0)
            }
            Phase::Up => (duty >= 25.0).then_some(500.0),
        });
        assert!(matches!(outcome, Step::Finished(result) if result.start_duty == 27.0));
    }

    fn inventory(ids: &[(&str, SensorKind)]) -> Inventory {
        Inventory {
            sensors: ids
                .iter()
                .map(|(id, kind)| SensorInfo {
                    id: id.to_string(),
                    label: id.to_string(),
                    kind: *kind,
                })
                .collect(),
            controls: Vec::new(),
        }
    }

    #[test]
    fn tach_pairing_prefers_an_exact_id_then_the_channel_number_on_the_same_device() {
        let inv = inventory(&[
            ("corsair/dev1/fan2", SensorKind::Rpm),
            ("hwmon/chipA/fan1", SensorKind::Rpm),
            ("hwmon/chipA/temp1", SensorKind::Temp),
            ("hwmon/chipB/fan1", SensorKind::Rpm),
        ]);
        assert_eq!(
            tach_for("corsair/dev1/fan2", &inv).as_deref(),
            Some("corsair/dev1/fan2")
        );
        assert_eq!(
            tach_for("hwmon/chipA/pwm1", &inv).as_deref(),
            Some("hwmon/chipA/fan1")
        );
        assert_eq!(tach_for("hwmon/chipA/pwm3", &inv), None);
        assert_eq!(tach_for("nvidia/0/fan0", &inv), None);
    }

    struct FakeActuator {
        duties: Mutex<Vec<f64>>,
        released: AtomicBool,
        persisted: Mutex<Option<CalibrationResult>>,
    }

    impl FakeActuator {
        fn new() -> Self {
            Self {
                duties: Mutex::new(Vec::new()),
                released: AtomicBool::new(false),
                persisted: Mutex::new(None),
            }
        }

        fn current(&self) -> f64 {
            *self.duties.lock().unwrap().last().unwrap()
        }

        fn came_from_rest(&self) -> bool {
            let duties = self.duties.lock().unwrap();
            duties.contains(&0.0)
        }
    }

    impl Actuator for FakeActuator {
        async fn set_duty(&self, duty: f64) -> Result<(), String> {
            self.duties.lock().unwrap().push(duty);
            Ok(())
        }

        async fn settle(&self) {}

        async fn rpm(&self) -> Option<f64> {
            let duty = self.current();
            if self.came_from_rest() {
                (duty >= 30.0).then_some(500.0)
            } else {
                (duty >= 20.0).then_some(600.0)
            }
        }

        async fn release(&self) {
            self.released.store(true, Ordering::SeqCst);
        }

        async fn persist(&self, result: CalibrationResult) -> Result<(), String> {
            *self.persisted.lock().unwrap() = Some(result);
            Ok(())
        }
    }

    #[tokio::test]
    async fn run_drives_the_actuator_persists_the_result_and_releases_the_control() {
        let actuator = FakeActuator::new();
        let calibrator = Calibrator::new();
        let cancel = calibrator.begin().unwrap();
        let outcome = run(
            &actuator,
            "hwmon/x/pwm1",
            "hwmon/x/fan1",
            calibrator.sender(),
            &cancel,
        )
        .await;
        match outcome {
            Progress::Done { result, .. } => {
                assert_eq!(result.min_duty, 22.0);
                assert_eq!(result.start_duty, 32.0);
                assert_eq!(result.stop_duty, 17.0);
            }
            other => panic!("unexpected outcome {other:?}"),
        }
        assert!(actuator.released.load(Ordering::SeqCst));
        assert!(actuator.persisted.lock().unwrap().is_some());
        assert!(matches!(calibrator.progress(), Progress::Done { .. }));
        assert!(!calibrator.is_running());
    }

    #[tokio::test]
    async fn cancel_stops_the_sweep_and_still_releases_the_control() {
        let actuator = FakeActuator::new();
        let calibrator = Calibrator::new();
        let cancel = calibrator.begin().unwrap();
        cancel.store(true, Ordering::SeqCst);
        let outcome = run(
            &actuator,
            "hwmon/x/pwm1",
            "hwmon/x/fan1",
            calibrator.sender(),
            &cancel,
        )
        .await;
        assert!(matches!(outcome, Progress::Failed { ref error, .. } if error == "cancelled"));
        assert!(actuator.released.load(Ordering::SeqCst));
        assert!(actuator.persisted.lock().unwrap().is_none());
    }

    #[test]
    fn only_one_calibration_runs_at_a_time() {
        let calibrator = Calibrator::new();
        let _cancel = calibrator.begin().unwrap();
        calibrator.sender().send_replace(Progress::Running {
            control: "c".into(),
            tach: "t".into(),
            phase: Phase::Probe,
            duty: 50.0,
            rpm: None,
        });
        assert!(calibrator.begin().is_err());
        assert!(calibrator.cancel());
        calibrator.sender().send_replace(Progress::Idle);
        assert!(!calibrator.cancel());
        assert!(calibrator.begin().is_ok());
    }
}
