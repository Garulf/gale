mod canonical;

use gale_hw::{Backend, ControlInfo, HwError, Id, Inventory, SensorInfo, SensorKind};
use std::collections::{HashMap, HashSet};
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

struct Chip {
    name: String,
    driver: String,
    dir: PathBuf,
}

struct FoundSensor {
    raw_id: Id,
    canonical_id: Option<Id>,
    label: String,
    entry: SensorEntry,
}

struct FoundControl {
    raw_id: Id,
    canonical_id: Option<Id>,
    label: String,
    pwm_path: PathBuf,
    enable_path: Option<PathBuf>,
}

#[derive(Default)]
struct Found {
    sensors: Vec<FoundSensor>,
    controls: Vec<FoundControl>,
}

impl Found {
    fn canonical_counts(&self) -> HashMap<Id, usize> {
        let mut counts = HashMap::new();
        let sensor_ids = self.sensors.iter().map(|s| &s.canonical_id);
        let control_ids = self.controls.iter().map(|c| &c.canonical_id);
        for id in sensor_ids.chain(control_ids).flatten() {
            *counts.entry(id.clone()).or_insert(0) += 1;
        }
        counts
    }
}

fn resolve_id(raw_id: Id, canonical_id: Option<Id>, counts: &HashMap<Id, usize>) -> Id {
    canonical_id
        .filter(|id| counts.get(id) == Some(&1))
        .unwrap_or(raw_id)
}

pub struct HwmonBackend {
    root: PathBuf,
    skipped_drivers: HashSet<String>,
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
            skipped_drivers: HashSet::new(),
            sensors: HashMap::new(),
            controls: HashMap::new(),
        }
    }

    /// Leaves out chips whose kernel driver serves a device another backend
    /// already drives, so one fan never shows up under two controls.
    pub fn skipping_drivers(mut self, drivers: impl IntoIterator<Item = String>) -> Self {
        self.skipped_drivers.extend(drivers);
        self
    }

    fn chips(&self) -> Vec<Chip> {
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
            let driver = name.trim().to_string();
            if self.skipped_drivers.contains(&driver) {
                continue;
            }
            let seen = counts.entry(driver.clone()).or_insert(0);
            let name = if *seen == 0 {
                driver.clone()
            } else {
                format!("{driver}-{seen}")
            };
            *seen += 1;
            chips.push(Chip { name, driver, dir });
        }
        chips
    }

    fn scan_chip(chip: &Chip, found: &mut Found) {
        let Ok(entries) = fs::read_dir(&chip.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if let Some(stem) = numbered_stem(&file, "temp", "_input") {
                found
                    .sensors
                    .push(Self::sensor(chip, &stem, &file, SensorKind::Temp));
            } else if let Some(stem) = numbered_stem(&file, "fan", "_input") {
                found
                    .sensors
                    .push(Self::sensor(chip, &stem, &file, SensorKind::Rpm));
            } else if is_pwm_file(&file) {
                let enable = chip.dir.join(format!("{file}_enable"));
                found.controls.push(FoundControl {
                    raw_id: format!("hwmon/{}/{file}", chip.name),
                    canonical_id: canonical::control_id(&chip.driver, &file),
                    label: format!("{} {file}", chip.name),
                    pwm_path: entry.path(),
                    enable_path: enable.exists().then_some(enable),
                });
            }
        }
    }

    fn sensor(chip: &Chip, stem: &str, file: &str, kind: SensorKind) -> FoundSensor {
        let label = fs::read_to_string(chip.dir.join(format!("{stem}_label")))
            .map(|l| l.trim().to_string())
            .ok();
        FoundSensor {
            raw_id: format!("hwmon/{}/{stem}", chip.name),
            canonical_id: canonical::sensor_id(
                &chip.driver,
                &chip.dir,
                stem,
                label.as_deref(),
                kind,
            ),
            label: label.unwrap_or_else(|| format!("{} {stem}", chip.name)),
            entry: SensorEntry {
                path: chip.dir.join(file),
                kind,
            },
        }
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
        let mut found = Found::default();
        for chip in self.chips() {
            Self::scan_chip(&chip, &mut found);
        }
        let counts = found.canonical_counts();
        let mut inventory = Inventory::default();
        for sensor in found.sensors {
            let id = resolve_id(sensor.raw_id, sensor.canonical_id, &counts);
            inventory.sensors.push(SensorInfo {
                id: id.clone(),
                label: sensor.label,
                kind: sensor.entry.kind,
            });
            self.sensors.insert(id, sensor.entry);
        }
        for control in found.controls {
            let id = resolve_id(control.raw_id, control.canonical_id, &counts);
            let saved_enable = previous
                .values()
                .find(|old| old.pwm_path == control.pwm_path)
                .and_then(|old| old.saved_enable.clone());
            self.controls.insert(
                id.clone(),
                ControlEntry {
                    pwm_path: control.pwm_path,
                    enable_path: control.enable_path,
                    saved_enable,
                },
            );
            inventory.controls.push(ControlInfo {
                id,
                label: control.label,
            });
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

    fn restore_hint(&self, id: &str) -> Option<(String, String)> {
        let control = self.controls.get(id)?;
        let enable_path = control.enable_path.as_ref()?;
        let saved = control.saved_enable.as_ref()?;
        Some((enable_path.display().to_string(), saved.clone()))
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
        write(&chip0, "name", "it8792\n");
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
                "hwmon/it8792/fan1",
                "hwmon/it8792/temp1"
            ]
        );
        let cpu_temp = inventory
            .sensors
            .iter()
            .find(|s| s.id == "hwmon/it8792/temp1")
            .unwrap();
        assert_eq!(cpu_temp.label, "CPUTIN");
        assert_eq!(cpu_temp.kind, SensorKind::Temp);
        let fan = inventory
            .sensors
            .iter()
            .find(|s| s.id == "hwmon/it8792/fan1")
            .unwrap();
        assert_eq!(fan.label, "it8792 fan1");
        assert_eq!(fan.kind, SensorKind::Rpm);
        let control_ids: Vec<_> = inventory.controls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(control_ids, vec!["hwmon/it8792/pwm1"]);
    }

    fn chip(root: &Path, dir: &str, name: &str) -> std::path::PathBuf {
        let chip = root.join(dir);
        fs::create_dir(&chip).unwrap();
        write(&chip, "name", &format!("{name}\n"));
        chip
    }

    #[cfg(unix)]
    fn i2c_client(root: &Path, bus: u32, address: u16, adapter: &str) -> std::path::PathBuf {
        let adapter_dir = root.join("devices").join(format!("i2c-{bus}"));
        let client = adapter_dir.join(format!("{bus}-{address:04x}"));
        fs::create_dir_all(&client).unwrap();
        write(&adapter_dir, "name", &format!("{adapter}\n"));
        client
    }

    fn sensor_ids(inventory: &Inventory) -> Vec<&str> {
        inventory.sensors.iter().map(|s| s.id.as_str()).collect()
    }

    #[test]
    #[cfg(unix)]
    fn known_chips_report_the_ids_windows_uses_for_the_same_hardware() {
        let tree = tempfile::tempdir().unwrap();
        let hwmon = tree.path().join("hwmon");
        fs::create_dir(&hwmon).unwrap();
        let nct = chip(&hwmon, "hwmon0", "nct6798");
        write(&nct, "temp1_input", "30000\n");
        write(&nct, "temp1_label", "SYSTIN\n");
        write(&nct, "temp7_input", "50000\n");
        write(&nct, "temp7_label", "PECI Agent 0\n");
        write(&nct, "fan2_input", "900\n");
        write(&nct, "pwm2", "100\n");
        let k10 = chip(&hwmon, "hwmon1", "k10temp");
        write(&k10, "temp1_input", "55000\n");
        write(&k10, "temp1_label", "Tctl\n");
        write(&k10, "temp3_input", "52000\n");
        write(&k10, "temp3_label", "Tccd1\n");
        let ec = chip(&hwmon, "hwmon2", "asusec");
        write(&ec, "temp1_input", "31000\n");
        write(&ec, "temp1_label", "Water_In\n");
        write(&ec, "fan1_input", "1500\n");
        write(&ec, "fan1_label", "CPU_Opt\n");
        let dimm3 = chip(&hwmon, "hwmon3", "spd5118");
        write(&dimm3, "temp1_input", "40000\n");
        let client = i2c_client(tree.path(), 3, 0x52, "SMBus PIIX4 adapter port 0 at 0b00");
        std::os::unix::fs::symlink(client, dimm3.join("device")).unwrap();
        let other_bus = chip(&hwmon, "hwmon4", "jc42");
        write(&other_bus, "temp1_input", "41000\n");
        let client = i2c_client(tree.path(), 4, 0x18, "SMBus PIIX4 adapter port 2 at 0b20");
        std::os::unix::fs::symlink(client, other_bus.join("device")).unwrap();

        let mut backend = HwmonBackend::with_root(hwmon);
        let inventory = backend.enumerate().unwrap();

        assert_eq!(
            sensor_ids(&inventory),
            vec![
                "cpu/amd/ccd1",
                "cpu/amd/tctl",
                "dimm/1-bus2/temp",
                "dimm/3/temp",
                "ec/asus/cpu_optional_fan",
                "ec/asus/water_in",
                "superio/nct6798d/fan2",
                "superio/nct6798d/temp1",
                "superio/nct6798d/temp3",
            ]
        );
        let controls: Vec<_> = inventory.controls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(controls, vec!["superio/nct6798d/pwm2"]);
        let values = backend.read_all();
        assert_eq!(values["superio/nct6798d/temp1"], Some(50.0));
        assert_eq!(values["cpu/amd/tctl"], Some(55.0));
        backend.set_duty("superio/nct6798d/pwm2", 100.0).unwrap();
        assert_eq!(read_file(&nct, "pwm2"), "255");
    }

    #[test]
    fn two_chips_that_would_share_an_id_keep_their_hwmon_ids() {
        let tree = tempfile::tempdir().unwrap();
        for dir in ["hwmon0", "hwmon1"] {
            let k10 = chip(tree.path(), dir, "k10temp");
            write(&k10, "temp1_input", "55000\n");
            write(&k10, "temp1_label", "Tctl\n");
        }
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        let inventory = backend.enumerate().unwrap();
        assert_eq!(
            sensor_ids(&inventory),
            vec!["hwmon/k10temp-1/temp1", "hwmon/k10temp/temp1"]
        );
    }

    #[test]
    fn labels_without_a_windows_counterpart_keep_their_hwmon_id() {
        let tree = tempfile::tempdir().unwrap();
        let nct = chip(tree.path(), "hwmon0", "nct6798");
        write(&nct, "temp9_input", "20000\n");
        write(&nct, "temp9_label", "Virtual_TEMP\n");
        let k10 = chip(tree.path(), "hwmon1", "k10temp");
        write(&k10, "temp2_input", "20000\n");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        let inventory = backend.enumerate().unwrap();
        assert_eq!(
            sensor_ids(&inventory),
            vec!["hwmon/k10temp/temp2", "hwmon/nct6798/temp9"]
        );
    }

    #[test]
    fn skipped_drivers_are_left_out() {
        let tree = mock_tree();
        let cpro = chip(tree.path(), "hwmon2", "corsaircpro");
        write(&cpro, "fan1_input", "800\n");
        write(&cpro, "pwm1", "100\n");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf())
            .skipping_drivers(["corsaircpro".to_string()]);
        let inventory = backend.enumerate().unwrap();
        assert!(!inventory
            .sensors
            .iter()
            .any(|s| s.id.contains("corsaircpro")));
        assert!(!inventory
            .controls
            .iter()
            .any(|c| c.id.contains("corsaircpro")));
    }

    #[test]
    fn duplicate_chip_names_get_suffixes_in_path_order() {
        let tree = mock_tree();
        let chip2 = tree.path().join("hwmon2");
        fs::create_dir(&chip2).unwrap();
        write(&chip2, "name", "it8792\n");
        write(&chip2, "temp1_input", "30000\n");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        let inventory = backend.enumerate().unwrap();
        assert!(inventory
            .sensors
            .iter()
            .any(|s| s.id == "hwmon/it8792/temp1"));
        assert!(inventory
            .sensors
            .iter()
            .any(|s| s.id == "hwmon/it8792-1/temp1"));
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
        assert_eq!(values["hwmon/it8792/temp1"], Some(45.0));
        assert_eq!(values["hwmon/amdgpu/temp1"], Some(60.0));
        assert_eq!(values["hwmon/it8792/fan1"], Some(1200.0));
        let duty = values["hwmon/it8792/pwm1"].unwrap();
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
        assert_eq!(values["hwmon/it8792/temp1"], None);
        assert_eq!(values["hwmon/it8792/fan1"], Some(1200.0));
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
        backend.set_duty("hwmon/it8792/pwm1", 60.0).unwrap();
        assert_eq!(read_file(&chip0, "pwm1"), "153");
        assert_eq!(read_file(&chip0, "pwm1_enable"), "1");
        backend.set_duty("hwmon/it8792/pwm1", 0.0).unwrap();
        assert_eq!(read_file(&chip0, "pwm1"), "0");
        backend.release("hwmon/it8792/pwm1").unwrap();
        assert_eq!(read_file(&chip0, "pwm1_enable"), "5");
        backend.release("hwmon/it8792/pwm1").unwrap();
        assert_eq!(read_file(&chip0, "pwm1_enable"), "5");
    }

    #[test]
    fn set_duty_clamps_out_of_range_percentages() {
        let tree = mock_tree();
        let chip0 = tree.path().join("hwmon0");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        backend.set_duty("hwmon/it8792/pwm1", 150.0).unwrap();
        assert_eq!(read_file(&chip0, "pwm1"), "255");
        backend.set_duty("hwmon/it8792/pwm1", -5.0).unwrap();
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
    fn a_control_whose_id_changes_on_reenumerate_still_restores_on_release() {
        let tree = tempfile::tempdir().unwrap();
        let nct = chip(tree.path(), "hwmon0", "nct6798");
        write(&nct, "pwm1", "128\n");
        write(&nct, "pwm1_enable", "5\n");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        backend.set_duty("superio/nct6798d/pwm1", 60.0).unwrap();
        let second = chip(tree.path(), "hwmon1", "nct6798");
        write(&second, "pwm1", "0\n");
        let inventory = backend.enumerate().unwrap();
        assert!(inventory
            .controls
            .iter()
            .any(|c| c.id == "hwmon/nct6798/pwm1"));
        backend.release("hwmon/nct6798/pwm1").unwrap();
        assert_eq!(read_file(&nct, "pwm1_enable"), "5");
    }

    #[test]
    fn reenumerate_preserves_claim_state_so_release_still_restores() {
        let tree = mock_tree();
        let chip0 = tree.path().join("hwmon0");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        backend.set_duty("hwmon/it8792/pwm1", 60.0).unwrap();
        assert_eq!(read_file(&chip0, "pwm1_enable"), "1");
        backend.enumerate().unwrap();
        backend.release("hwmon/it8792/pwm1").unwrap();
        assert_eq!(read_file(&chip0, "pwm1_enable"), "5");
    }

    #[test]
    fn set_duty_rejects_non_finite_percentages() {
        let tree = mock_tree();
        let chip0 = tree.path().join("hwmon0");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        let result = backend.set_duty("hwmon/it8792/pwm1", f64::NAN);
        assert!(matches!(result, Err(HwError::Io { .. })));
        assert_eq!(read_file(&chip0, "pwm1"), "128");
        assert_eq!(read_file(&chip0, "pwm1_enable"), "5");
    }

    #[test]
    fn restore_hint_returns_enable_path_and_saved_value_after_claim() {
        let tree = mock_tree();
        let chip0 = tree.path().join("hwmon0");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        assert_eq!(backend.restore_hint("hwmon/it8792/pwm1"), None);
        backend.set_duty("hwmon/it8792/pwm1", 60.0).unwrap();
        let (path, value) = backend.restore_hint("hwmon/it8792/pwm1").unwrap();
        assert_eq!(path, chip0.join("pwm1_enable").display().to_string());
        assert_eq!(value, "5");
    }

    #[test]
    fn restore_hint_is_none_for_control_without_enable_file() {
        let tree = mock_tree();
        let chip1 = tree.path().join("hwmon1");
        write(&chip1, "pwm1", "0\n");
        let mut backend = HwmonBackend::with_root(tree.path().to_path_buf());
        backend.enumerate().unwrap();
        backend.set_duty("hwmon/amdgpu/pwm1", 100.0).unwrap();
        assert_eq!(backend.restore_hint("hwmon/amdgpu/pwm1"), None);
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
