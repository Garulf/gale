use crate::{Backend, HwError, Id, Inventory};
use std::collections::HashMap;

pub struct CompositeBackend {
    backends: Vec<Box<dyn Backend>>,
    owners: HashMap<Id, usize>,
}

impl CompositeBackend {
    pub fn new(backends: Vec<Box<dyn Backend>>) -> Self {
        Self { backends, owners: HashMap::new() }
    }

    fn owner_of(&mut self, id: &str) -> Result<&mut Box<dyn Backend>, HwError> {
        let index = *self.owners.get(id).ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        Ok(&mut self.backends[index])
    }
}

impl Backend for CompositeBackend {
    fn name(&self) -> &str {
        "composite"
    }

    fn enumerate(&mut self) -> Result<Inventory, HwError> {
        let mut merged = Inventory::default();
        self.owners.clear();
        for (index, backend) in self.backends.iter_mut().enumerate() {
            let Ok(inventory) = backend.enumerate() else {
                continue;
            };
            for control in &inventory.controls {
                self.owners.insert(control.id.clone(), index);
            }
            merged.sensors.extend(inventory.sensors);
            merged.controls.extend(inventory.controls);
        }
        Ok(merged)
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        let mut merged = HashMap::new();
        for backend in &mut self.backends {
            merged.extend(backend.read_all());
        }
        merged
    }

    fn set_duty(&mut self, id: &str, pct: f64) -> Result<(), HwError> {
        self.owner_of(id)?.set_duty(id, pct)
    }

    fn release(&mut self, id: &str) -> Result<(), HwError> {
        self.owner_of(id)?.release(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Backend, ControlInfo, HwError, Id, Inventory, SensorInfo, SensorKind};
    use std::collections::HashMap;

    struct Child {
        name: &'static str,
        fail_enumerate: bool,
        last_set: Option<(Id, f64)>,
        released: Vec<Id>,
    }

    impl Child {
        fn new(name: &'static str) -> Self {
            Self { name, fail_enumerate: false, last_set: None, released: Vec::new() }
        }
    }

    impl Backend for Child {
        fn name(&self) -> &str {
            self.name
        }

        fn enumerate(&mut self) -> Result<Inventory, HwError> {
            if self.fail_enumerate {
                return Err(HwError::Io { path: self.name.into(), message: "boom".into() });
            }
            Ok(Inventory {
                sensors: vec![SensorInfo {
                    id: format!("{}/t1", self.name),
                    label: "t1".into(),
                    kind: SensorKind::Temp,
                }],
                controls: vec![ControlInfo {
                    id: format!("{}/p1", self.name),
                    label: "p1".into(),
                }],
            })
        }

        fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
            [(format!("{}/t1", self.name), Some(1.0))].into()
        }

        fn set_duty(&mut self, id: &str, pct: f64) -> Result<(), HwError> {
            self.last_set = Some((id.to_string(), pct));
            Ok(())
        }

        fn release(&mut self, id: &str) -> Result<(), HwError> {
            self.released.push(id.to_string());
            Ok(())
        }
    }

    #[test]
    fn merges_inventories_and_read_all_across_children() {
        let mut composite =
            CompositeBackend::new(vec![Box::new(Child::new("a")), Box::new(Child::new("b"))]);
        let inventory = composite.enumerate().unwrap();
        let sensor_ids: Vec<_> = inventory.sensors.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(sensor_ids, vec!["a/t1", "b/t1"]);
        assert_eq!(inventory.controls.len(), 2);
        let values = composite.read_all();
        assert_eq!(values["a/t1"], Some(1.0));
        assert_eq!(values["b/t1"], Some(1.0));
    }

    #[test]
    fn routes_writes_to_owning_child_and_rejects_unknown() {
        let mut composite =
            CompositeBackend::new(vec![Box::new(Child::new("a")), Box::new(Child::new("b"))]);
        composite.enumerate().unwrap();
        composite.set_duty("b/p1", 40.0).unwrap();
        composite.release("b/p1").unwrap();
        assert!(matches!(composite.set_duty("ghost/p1", 1.0), Err(HwError::UnknownId(_))));
        assert!(matches!(composite.release("ghost/p1"), Err(HwError::UnknownId(_))));
    }

    #[test]
    fn failing_child_is_skipped_not_fatal() {
        let mut broken = Child::new("broken");
        broken.fail_enumerate = true;
        let mut composite =
            CompositeBackend::new(vec![Box::new(broken), Box::new(Child::new("ok"))]);
        let inventory = composite.enumerate().unwrap();
        assert_eq!(inventory.sensors.len(), 1);
        assert_eq!(inventory.sensors[0].id, "ok/t1");
        composite.set_duty("ok/p1", 10.0).unwrap();
    }
}
