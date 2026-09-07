use gale_hw::{Inventory, SensorKind};
use serde::Serialize;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tokio::task::AbortHandle;

pub const STEP_PCT: f64 = 5.0;
pub const PROBE_PCT: f64 = 50.0;
pub const MARGIN_PCT: f64 = 2.0;
pub const SETTLE_SECS: u64 = 4;
const MAX_REST_WAITS: u8 = 6;
const STALL_CONFIRMATIONS: u8 = 2;

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
    down_zeros: u8,
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
            down_zeros: 0,
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
                    self.down_zeros = 0;
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
                self.down_zeros += 1;
                if self.down_zeros < STALL_CONFIRMATIONS {
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
                    if self.rest_checks > MAX_REST_WAITS {
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
    let end = leaf
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_ascii_digit())
        .map(|(i, c)| i + c.len_utf8())?;
    let start = leaf[..end]
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    Some(&leaf[start..end])
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
    Cancelled {
        control: String,
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
    calibrator: &Calibrator,
    cancel: &AtomicBool,
) -> Progress {
    let progress = calibrator.sender();
    let mut sweep = Sweep::new();
    let mut step = sweep.first();
    let outcome = loop {
        match step {
            Step::SetDuty(duty) => {
                if cancel.load(Ordering::SeqCst) {
                    break Progress::Cancelled {
                        control: control.to_string(),
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
    calibrator.finish(outcome.clone());
    outcome
}

pub struct Calibrator {
    progress: watch::Sender<Progress>,
    busy: AtomicBool,
    active: Mutex<Option<Arc<AtomicBool>>>,
    task: Mutex<Option<AbortHandle>>,
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
            busy: AtomicBool::new(false),
            active: Mutex::new(None),
            task: Mutex::new(None),
        }
    }

    pub fn progress(&self) -> Progress {
        self.progress.borrow().clone()
    }

    pub fn is_running(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    pub fn begin(&self, control: &str, tach: &str) -> Result<Arc<AtomicBool>, String> {
        let mut active = self.active.lock().unwrap();
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("a calibration is already running".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *active = Some(cancel.clone());
        self.progress.send_replace(Progress::Running {
            control: control.to_string(),
            tach: tach.to_string(),
            phase: Phase::Probe,
            duty: PROBE_PCT,
            rpm: None,
        });
        Ok(cancel)
    }

    pub fn track(&self, task: AbortHandle) {
        *self.task.lock().unwrap() = Some(task);
    }

    pub fn sender(&self) -> &watch::Sender<Progress> {
        &self.progress
    }

    pub fn finish(&self, outcome: Progress) {
        *self.active.lock().unwrap() = None;
        *self.task.lock().unwrap() = None;
        self.busy.store(false, Ordering::SeqCst);
        self.progress.send_replace(outcome);
    }

    pub fn cancel(&self) -> bool {
        match self.active.lock().unwrap().as_ref() {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }

    pub fn shutdown(&self) {
        if let Some(flag) = self.active.lock().unwrap().take() {
            flag.store(true, Ordering::SeqCst);
        }
        if let Some(task) = self.task.lock().unwrap().take() {
            task.abort();
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
    fn a_single_zero_reading_on_the_way_down_does_not_end_the_sweep() {
        let mut sweep = Sweep::new();
        let glitched = AtomicBool::new(false);
        let (outcome, duties) = drive(&mut sweep, |phase, duty| match phase {
            Phase::Probe => Some(600.0),
            Phase::Down => {
                if duty == 45.0 && !glitched.swap(true, Ordering::SeqCst) {
                    return None;
                }
                (duty >= 20.0).then_some(600.0)
            }
            Phase::Rest => None,
            Phase::Up => (duty >= 30.0).then_some(500.0),
        });
        assert_eq!(
            outcome,
            Step::Finished(CalibrationResult {
                min_duty: 22.0,
                start_duty: 32.0,
                stop_duty: 17.0,
            }),
            "one zero reading must be re-checked, not taken as the stall"
        );
        assert_eq!(
            duties.iter().filter(|duty| **duty == 45.0).count(),
            2,
            "the glitched step must be probed a second time before the sweep moves on"
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

    #[test]
    fn channel_pairing_survives_non_ascii_control_ids() {
        let inv = inventory(&[
            ("hwmon/l\u{00fc}fter/fan2", SensorKind::Rpm),
            ("hwmon/l\u{00fc}fter/temp1", SensorKind::Temp),
        ]);
        assert_eq!(
            tach_for("hwmon/l\u{00fc}fter/pwm2\u{00b0}", &inv).as_deref(),
            Some("hwmon/l\u{00fc}fter/fan2")
        );
        assert_eq!(tach_for("hwmon/l\u{00fc}fter/pwm\u{00b0}", &inv), None);
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
        let cancel = calibrator.begin("hwmon/x/pwm1", "hwmon/x/fan1").unwrap();
        let outcome = run(
            &actuator,
            "hwmon/x/pwm1",
            "hwmon/x/fan1",
            &calibrator,
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
        let cancel = calibrator.begin("hwmon/x/pwm1", "hwmon/x/fan1").unwrap();
        cancel.store(true, Ordering::SeqCst);
        let outcome = run(
            &actuator,
            "hwmon/x/pwm1",
            "hwmon/x/fan1",
            &calibrator,
            &cancel,
        )
        .await;
        assert!(
            matches!(outcome, Progress::Cancelled { ref control } if control == "hwmon/x/pwm1")
        );
        assert!(matches!(calibrator.progress(), Progress::Cancelled { .. }));
        assert!(actuator.released.load(Ordering::SeqCst));
        assert!(actuator.persisted.lock().unwrap().is_none());
        assert!(!calibrator.is_running());
    }

    #[test]
    fn only_one_calibration_runs_at_a_time() {
        let calibrator = Calibrator::new();
        let _cancel = calibrator.begin("c", "t").unwrap();
        assert!(calibrator.is_running());
        assert!(matches!(calibrator.progress(), Progress::Running { .. }));
        assert!(calibrator.begin("c", "t").is_err());
        assert!(calibrator.cancel());
        calibrator.finish(Progress::Cancelled {
            control: "c".into(),
        });
        assert!(!calibrator.cancel());
        assert!(calibrator.begin("c", "t").is_ok());
    }

    #[test]
    fn begin_publishes_running_under_the_same_lock_it_claims() {
        let calibrator = Arc::new(Calibrator::new());
        let claims = Arc::new(AtomicUsize::new(0));
        let running_when_claimed = Arc::new(AtomicUsize::new(0));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let calibrator = calibrator.clone();
                let claims = claims.clone();
                let running_when_claimed = running_when_claimed.clone();
                std::thread::spawn(move || {
                    if calibrator.begin("c", "t").is_ok() {
                        claims.fetch_add(1, Ordering::SeqCst);
                        if matches!(calibrator.progress(), Progress::Running { .. }) {
                            running_when_claimed.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(claims.load(Ordering::SeqCst), 1);
        assert_eq!(running_when_claimed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn shutdown_cancels_the_sweep_and_aborts_the_tracked_task() {
        let calibrator = Calibrator::new();
        let cancel = calibrator.begin("hwmon/x/pwm1", "hwmon/x/fan1").unwrap();
        let task = tokio::spawn(std::future::pending::<()>());
        calibrator.track(task.abort_handle());
        calibrator.shutdown();
        assert!(cancel.load(Ordering::SeqCst));
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!calibrator.cancel());
    }
}
