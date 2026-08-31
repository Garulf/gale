use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type Id = String;

#[derive(Debug, thiserror::Error)]
pub enum HwError {
    #[error("unknown control id: {0}")]
    UnknownId(Id),
    #[error("io error on {path}: {message}")]
    Io { path: String, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorKind {
    Temp,
    Rpm,
    Duty,
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
                controls: vec![ControlInfo { id: "fake/p1".into(), label: "p1".into() }],
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
            self.duties.remove(id).map(|_| ()).ok_or_else(|| HwError::UnknownId(id.to_string()))
        }
    }

    #[test]
    fn backend_is_usable_as_send_trait_object() {
        fn assert_send<T: Send + ?Sized>() {}
        assert_send::<dyn Backend>();
        let mut backend: Box<dyn Backend> = Box::new(FakeBackend { duties: HashMap::new() });
        assert_eq!(backend.name(), "fake");
        let inventory = backend.enumerate().unwrap();
        assert_eq!(inventory.sensors[0].kind, SensorKind::Temp);
        assert_eq!(backend.read_all()["fake/t1"], Some(42.0));
        backend.set_duty("fake/p1", 55.0).unwrap();
        backend.release("fake/p1").unwrap();
        assert!(matches!(backend.set_duty("fake/nope", 1.0), Err(HwError::UnknownId(_))));
    }

    #[test]
    fn sensor_kind_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&SensorKind::Temp).unwrap(), "\"temp\"");
    }
}
