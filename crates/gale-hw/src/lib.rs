use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod composite;

pub type Id = String;

#[derive(Debug, thiserror::Error)]
pub enum HwError {
    #[error("unknown control id: {0}")]
    UnknownId(Id),
    #[error("io error on {path}: {message}")]
    Io { path: String, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorKind {
    Temp,
    Rpm,
    Duty,
    Percent,
    Clock,
    Memory,
    Power,
    State,
}

impl SensorKind {
    pub const ALL: [SensorKind; 8] = [
        SensorKind::Temp,
        SensorKind::Rpm,
        SensorKind::Duty,
        SensorKind::Percent,
        SensorKind::Clock,
        SensorKind::Memory,
        SensorKind::Power,
        SensorKind::State,
    ];

    pub fn unit(self) -> &'static str {
        match self {
            SensorKind::Temp => "°C",
            SensorKind::Rpm => "rpm",
            SensorKind::Duty | SensorKind::Percent => "%",
            SensorKind::Clock => "MHz",
            SensorKind::Memory => "MiB",
            SensorKind::Power => "W",
            SensorKind::State => "",
        }
    }

    pub fn is_curve_input(self) -> bool {
        !matches!(self, SensorKind::Rpm | SensorKind::Duty)
    }
}

#[cfg(test)]
mod kind_tests {
    use super::SensorKind;

    #[test]
    fn kinds_round_trip_as_snake_case_and_carry_units() {
        for kind in SensorKind::ALL {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(serde_json::from_str::<SensorKind>(&json).unwrap(), kind);
        }
        assert_eq!(
            serde_json::to_string(&SensorKind::Percent).unwrap(),
            "\"percent\""
        );
        assert_eq!(
            serde_json::to_string(&SensorKind::State).unwrap(),
            "\"state\""
        );
        assert_eq!(SensorKind::Power.unit(), "W");
        assert_eq!(SensorKind::Clock.unit(), "MHz");
        assert_eq!(SensorKind::Memory.unit(), "MiB");
        assert_eq!(SensorKind::State.unit(), "");
    }

    #[test]
    fn only_rpm_and_duty_are_not_curve_inputs() {
        let inputs: Vec<_> = SensorKind::ALL
            .into_iter()
            .filter(|k| k.is_curve_input())
            .collect();
        assert_eq!(inputs.len(), 6);
        assert!(!SensorKind::Rpm.is_curve_input());
        assert!(!SensorKind::Duty.is_curve_input());
        assert!(SensorKind::Temp.is_curve_input());
        assert!(SensorKind::State.is_curve_input());
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SensorInfo {
    pub id: Id,
    pub label: String,
    pub kind: SensorKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlInfo {
    pub id: Id,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    pub sensors: Vec<SensorInfo>,
    pub controls: Vec<ControlInfo>,
}

pub trait Backend: Send {
    fn name(&self) -> &str;
    fn enumerate(&mut self) -> Result<Inventory, HwError>;
    fn read_all(&mut self) -> HashMap<Id, Option<f64>>;
    fn set_duty(&mut self, id: &str, pct: f64) -> Result<(), HwError>;
    fn release(&mut self, id: &str) -> Result<(), HwError>;
    fn restore_hint(&self, _id: &str) -> Option<(String, String)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeBackend {
        duties: HashMap<Id, f64>,
    }

    impl Backend for FakeBackend {
        fn name(&self) -> &str {
            "fake"
        }

        fn enumerate(&mut self) -> Result<Inventory, HwError> {
            Ok(Inventory {
                sensors: vec![SensorInfo {
                    id: "fake/t1".into(),
                    label: "t1".into(),
                    kind: SensorKind::Temp,
                }],
                controls: vec![ControlInfo {
                    id: "fake/p1".into(),
                    label: "p1".into(),
                }],
            })
        }

        fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
            [("fake/t1".to_string(), Some(42.0))].into()
        }

        fn set_duty(&mut self, id: &str, pct: f64) -> Result<(), HwError> {
            if id != "fake/p1" {
                return Err(HwError::UnknownId(id.to_string()));
            }
            self.duties.insert(id.to_string(), pct);
            Ok(())
        }

        fn release(&mut self, id: &str) -> Result<(), HwError> {
            self.duties
                .remove(id)
                .map(|_| ())
                .ok_or_else(|| HwError::UnknownId(id.to_string()))
        }
    }

    #[test]
    fn backend_is_usable_as_send_trait_object() {
        fn assert_send<T: Send + ?Sized>() {}
        assert_send::<dyn Backend>();
        let mut backend: Box<dyn Backend> = Box::new(FakeBackend {
            duties: HashMap::new(),
        });
        assert_eq!(backend.name(), "fake");
        let inventory = backend.enumerate().unwrap();
        assert_eq!(inventory.sensors[0].kind, SensorKind::Temp);
        assert_eq!(backend.read_all()["fake/t1"], Some(42.0));
        backend.set_duty("fake/p1", 55.0).unwrap();
        backend.release("fake/p1").unwrap();
        assert!(matches!(
            backend.set_duty("fake/nope", 1.0),
            Err(HwError::UnknownId(_))
        ));
    }

    #[test]
    fn sensor_kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&SensorKind::Temp).unwrap(),
            "\"temp\""
        );
    }
}
