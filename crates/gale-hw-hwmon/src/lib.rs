use gale_hw::{Backend, ControlInfo, HwError, Id, Inventory, SensorInfo, SensorKind};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

struct SensorEntry {
    path: PathBuf,
    kind: SensorKind,
}

struct ControlEntry {
    pwm_path: PathBuf,
    enable_path: Option<PathBuf>,
    saved_enable: Option<String>,
}

pub struct HwmonBackend {
    root: PathBuf,
    sensors: HashMap<Id, SensorEntry>,
    controls: HashMap<Id, ControlEntry>,
}

impl HwmonBackend {
    pub fn new() -> Self {
        let root = std::env::var("GALE_HWMON_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/sys/class/hwmon"));
        Self::with_root(root)
    }

    pub fn with_root(root: PathBuf) -> Self {
        Self {
            root,
            sensors: HashMap::new(),
            controls: HashMap::new(),
        }
    }

    fn chips(&self) -> Vec<(String, PathBuf)> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        let mut counts: HashMap<String, u32> = HashMap::new();
        let mut chips = Vec::new();
        for dir in dirs {
            let Ok(name) = fs::read_to_string(dir.join("name")) else {
                continue;
            };
            let base = name.trim().to_string();
            let seen = counts.entry(base.clone()).or_insert(0);
            let chip_name = if *seen == 0 {
                base.clone()
            } else {
                format!("{base}-{seen}")
            };
            *seen += 1;
            chips.push((chip_name, dir));
        }
        chips
    }

    fn scan_chip(
        &mut self,
        chip: &str,
        dir: &Path,
        inventory: &mut Inventory,
        previous: &HashMap<Id, ControlEntry>,
    ) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if let Some(stem) = numbered_stem(&file, "temp", "_input") {
                self.add_sensor(chip, dir, &stem, &file, SensorKind::Temp, inventory);
            } else if let Some(stem) = numbered_stem(&file, "fan", "_input") {
                self.add_sensor(chip, dir, &stem, &file, SensorKind::Rpm, inventory);
            } else if is_pwm_file(&file) {
                let id = format!("hwmon/{chip}/{file}");
                let enable = dir.join(format!("{file}_enable"));
                let pwm_path = entry.path();
                let saved_enable = previous
                    .get(&id)
                    .filter(|old| old.pwm_path == pwm_path)
                    .and_then(|old| old.saved_enable.clone());
                self.controls.insert(
                    id.clone(),
                    ControlEntry {
                        pwm_path,
                        enable_path: enable.exists().then_some(enable),
                        saved_enable,
                    },
                );
                inventory.controls.push(ControlInfo {
                    id,
                    label: format!("{chip} {file}"),
                });
            }
        }
    }

    fn add_sensor(
        &mut self,
        chip: &str,
        dir: &Path,
        stem: &str,
        file: &str,
        kind: SensorKind,
        inventory: &mut Inventory,
    ) {
        let id = format!("hwmon/{chip}/{stem}");
        let label = fs::read_to_string(dir.join(format!("{stem}_label")))
            .map(|l| l.trim().to_string())
            .unwrap_or_else(|_| format!("{chip} {stem}"));
        self.sensors.insert(
            id.clone(),
            SensorEntry {
                path: dir.join(file),
                kind,
            },
        );
        inventory.sensors.push(SensorInfo { id, label, kind });
    }
}

impl Default for HwmonBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn numbered_stem(file: &str, prefix: &str, suffix: &str) -> Option<String> {
    let stem = file.strip_suffix(suffix)?;
    let digits = stem.strip_prefix(prefix)?;
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then(|| stem.to_string())
}

fn io_error(path: &Path, error: &std::io::Error) -> HwError {
    HwError::Io {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

fn read_number(path: &Path) -> Option<f64> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn is_pwm_file(file: &str) -> bool {
    file.strip_prefix("pwm")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

impl Backend for HwmonBackend {
    fn name(&self) -> &str {
        "hwmon"
    }

    fn enumerate(&mut self) -> Result<Inventory, HwError> {
        self.sensors.clear();
        let previous = std::mem::take(&mut self.controls);
        let mut inventory = Inventory::default();
        for (chip, dir) in self.chips() {
            self.scan_chip(&chip, &dir, &mut inventory, &previous);
        }
        inventory.sensors.sort_by(|a, b| a.id.cmp(&b.id));
        inventory.controls.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(inventory)
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        let mut values = HashMap::new();
        for (id, sensor) in &self.sensors {
            let raw = read_number(&sensor.path);
            let value = match sensor.kind {
                SensorKind::Temp => raw.map(|v| v / 1000.0),
                _ => raw,
            };
            values.insert(id.clone(), value);
        }
        for (id, control) in &self.controls {
            let value = read_number(&control.pwm_path).map(|v| v / 255.0 * 100.0);
            values.insert(id.clone(), value);
        }
        values
    }

    fn set_duty(&mut self, id: &str, pct: f64) -> Result<(), HwError> {
        let control = self
            .controls
            .get_mut(id)
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        if !pct.is_finite() {
            return Err(HwError::Io {
                path: control.pwm_path.display().to_string(),
                message: "non-finite duty".into(),
            });
        }
        if control.saved_enable.is_none() {
            if let Some(enable_path) = &control.enable_path {
                let current =
                    fs::read_to_string(enable_path).map_err(|e| io_error(enable_path, &e))?;
                fs::write(enable_path, "1").map_err(|e| io_error(enable_path, &e))?;
                control.saved_enable = Some(current.trim().to_string());
            }
        }
        let raw = (pct.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u32;
        fs::write(&control.pwm_path, raw.to_string()).map_err(|e| io_error(&control.pwm_path, &e))
    }

    fn release(&mut self, id: &str) -> Result<(), HwError> {
        let control = self
            .controls
            .get_mut(id)
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        if let Some(saved) = control.saved_enable.take() {
            if let Some(enable_path) = &control.enable_path {
                fs::write(enable_path, saved).map_err(|e| io_error(enable_path, &e))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_hw::{Backend, SensorKind};
    use std::fs;
    use std::path::Path;

    fn write(dir: &Path, file: &str, contents: &str) {
        fs::write(dir.join(file), contents).unwrap();
    }

    fn mock_tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let chip0 = root.path().join("hwmon0");
        fs::create_dir(&chip0).unwrap();
        write(&chip0, "name", "nct6798\n");
        write(&chip0, "temp1_input", "45000\n");
        write(&chip0, "temp1_label", "CPUTIN\n");
        write(&chip0, "fan1_input", "1200\n");
        write(&chip0, "pwm1", "128\n");
        write(&chip0, "pwm1_enable", "5\n");
        write(&chip0, "pwm1_mode", "1\n");
        let chip1 = root.path().join("hwmon1");
        fs::create_dir(&chip1).unwrap();
        write(&chip1, "name", "amdgpu\n");
        write(&chip1, "temp1_input", "60000\n");
        root
    }

    #[test]
    fn enumerates_chips_sensors_and_controls_with_stable_ids() {
        let tree = mock_tree();
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        let inventory = backend.enumerate().unwrap();
        let sensor_ids: Vec<_> = inventory.sensors.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            sensor_ids,
            vec![
                "hwmon/amdgpu/temp1",
                "hwmon/nct6798/fan1",
                "hwmon/nct6798/temp1"
            ]
        );
        let cpu_temp = inventory
            .sensors
            .iter()
            .find(|s| s.id == "hwmon/nct6798/temp1")
            .unwrap();
        assert_eq!(cpu_temp.label, "CPUTIN");
        assert_eq!(cpu_temp.kind, SensorKind::Temp);
        let fan = inventory
            .sensors
            .iter()
            .find(|s| s.id == "hwmon/nct6798/fan1")
            .unwrap();
        assert_eq!(fan.label, "nct6798 fan1");
        assert_eq!(fan.kind, SensorKind::Rpm);
        let control_ids: Vec<_> = inventory.controls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(control_ids, vec!["hwmon/nct6798/pwm1"]);
    }

    #[test]
    fn duplicate_chip_names_get_suffixes_in_path_order() {
        let tree = mock_tree();
        let chip2 = tree.path().join("hwmon2");
        fs::create_dir(&chip2).unwrap();
        write(&chip2, "name", "nct6798\n");
        write(&chip2, "temp1_input", "30000\n");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        let inventory = backend.enumerate().unwrap();
        assert!(inventory
            .sensors
            .iter()
            .any(|s| s.id == "hwmon/nct6798/temp1"));
        assert!(inventory
            .sensors
            .iter()
            .any(|s| s.id == "hwmon/nct6798-1/temp1"));
    }

    #[test]
    fn enumerate_is_rerunnable_without_duplicates() {
        let tree = mock_tree();
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        let first = backend.enumerate().unwrap();
        let second = backend.enumerate().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn directories_without_name_file_are_ignored() {
        let tree = mock_tree();
        fs::create_dir(tree.path().join("subsystem")).unwrap();
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        let inventory = backend.enumerate().unwrap();
        assert_eq!(inventory.controls.len(), 1);
        assert_eq!(inventory.sensors.len(), 3);
    }

    #[test]
    fn read_all_converts_units_and_covers_controls() {
        let tree = mock_tree();
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["hwmon/nct6798/temp1"], Some(45.0));
        assert_eq!(values["hwmon/amdgpu/temp1"], Some(60.0));
        assert_eq!(values["hwmon/nct6798/fan1"], Some(1200.0));
        let duty = values["hwmon/nct6798/pwm1"].unwrap();
        assert!((duty - 50.196).abs() < 0.01);
        assert_eq!(values.len(), 4);
    }

    #[test]
    fn unreadable_sensor_reads_none_not_zero() {
        let tree = mock_tree();
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        fs::remove_file(tree.path().join("hwmon1").join("temp1_input")).unwrap();
        write(&tree.path().join("hwmon0"), "temp1_input", "garbage\n");
        let values = backend.read_all();
        assert_eq!(values["hwmon/amdgpu/temp1"], None);
        assert_eq!(values["hwmon/nct6798/temp1"], None);
        assert_eq!(values["hwmon/nct6798/fan1"], Some(1200.0));
    }

    fn read_file(dir: &Path, file: &str) -> String {
        fs::read_to_string(dir.join(file))
            .unwrap()
            .trim()
            .to_string()
    }

    #[test]
    fn set_duty_claims_enable_scales_and_release_restores() {
        let tree = mock_tree();
        let chip0 = tree.path().join("hwmon0");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        backend.set_duty("hwmon/nct6798/pwm1", 60.0).unwrap();
        assert_eq!(read_file(&chip0, "pwm1"), "153");
        assert_eq!(read_file(&chip0, "pwm1_enable"), "1");
        backend.set_duty("hwmon/nct6798/pwm1", 0.0).unwrap();
        assert_eq!(read_file(&chip0, "pwm1"), "0");
        backend.release("hwmon/nct6798/pwm1").unwrap();
        assert_eq!(read_file(&chip0, "pwm1_enable"), "5");
        backend.release("hwmon/nct6798/pwm1").unwrap();
        assert_eq!(read_file(&chip0, "pwm1_enable"), "5");
    }

    #[test]
    fn set_duty_clamps_out_of_range_percentages() {
        let tree = mock_tree();
        let chip0 = tree.path().join("hwmon0");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        backend.set_duty("hwmon/nct6798/pwm1", 150.0).unwrap();
        assert_eq!(read_file(&chip0, "pwm1"), "255");
        backend.set_duty("hwmon/nct6798/pwm1", -5.0).unwrap();
        assert_eq!(read_file(&chip0, "pwm1"), "0");
    }

    #[test]
    fn control_without_enable_file_still_writes_duty() {
        let tree = mock_tree();
        let chip1 = tree.path().join("hwmon1");
        write(&chip1, "pwm1", "0\n");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        backend.set_duty("hwmon/amdgpu/pwm1", 100.0).unwrap();
        assert_eq!(read_file(&chip1, "pwm1"), "255");
        backend.release("hwmon/amdgpu/pwm1").unwrap();
    }

    #[test]
    fn reenumerate_preserves_claim_state_so_release_still_restores() {
        let tree = mock_tree();
        let chip0 = tree.path().join("hwmon0");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        backend.set_duty("hwmon/nct6798/pwm1", 60.0).unwrap();
        assert_eq!(read_file(&chip0, "pwm1_enable"), "1");
        backend.enumerate().unwrap();
        backend.release("hwmon/nct6798/pwm1").unwrap();
        assert_eq!(read_file(&chip0, "pwm1_enable"), "5");
    }

    #[test]
    fn set_duty_rejects_non_finite_percentages() {
        let tree = mock_tree();
        let chip0 = tree.path().join("hwmon0");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        let result = backend.set_duty("hwmon/nct6798/pwm1", f64::NAN);
        assert!(matches!(result, Err(HwError::Io { .. })));
        assert_eq!(read_file(&chip0, "pwm1"), "128");
        assert_eq!(read_file(&chip0, "pwm1_enable"), "5");
    }

    #[test]
    fn unknown_control_ids_are_rejected() {
        let tree = mock_tree();
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        assert!(matches!(
            backend.set_duty("hwmon/ghost/pwm9", 10.0),
            Err(gale_hw::HwError::UnknownId(_))
        ));
        assert!(matches!(
            backend.release("hwmon/ghost/pwm9"),
            Err(gale_hw::HwError::UnknownId(_))
        ));
    }
}
