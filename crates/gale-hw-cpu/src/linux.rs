use std::fs;
use std::path::{Path, PathBuf};

use crate::backend::{CpuFreq, CpuSources, CpuTimes, ProcStat, Rapl};

pub const ROOT_ENV: &str = "GALE_CPU_ROOT";

const GUEST_FREE_FIELDS: usize = 8;
const RAPL_PACKAGE_NAME_PREFIX: &str = "package";
const RAPL_ZONES: [&str; 2] = [
    "sys/class/powercap/intel-rapl:0",
    "sys/class/powercap/intel-rapl-mmio:0",
];

pub fn root() -> PathBuf {
    std::env::var(ROOT_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

pub fn sources() -> CpuSources {
    sources_under(&root())
}

pub fn sources_under(root: &Path) -> CpuSources {
    CpuSources {
        usage: Some(Box::new(ProcStatFile::new(root))),
        clock: Some(Box::new(CpuFreqSysfs::new(root))),
        power: RaplSysfs::discover(root).map(|rapl| Box::new(rapl) as Box<dyn Rapl>),
    }
}

fn read_trimmed(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map(|text| text.trim().to_string())
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn read_u64(path: &Path) -> Result<u64, String> {
    read_trimmed(path)?
        .parse::<u64>()
        .map_err(|error| format!("{}: {error}", path.display()))
}

pub struct ProcStatFile {
    path: PathBuf,
}

impl ProcStatFile {
    pub fn new(root: &Path) -> Self {
        Self {
            path: root.join("proc/stat"),
        }
    }
}

pub fn parse_proc_stat(text: &str) -> Option<CpuTimes> {
    let line = text
        .lines()
        .find(|line| line.split_whitespace().next() == Some("cpu"))?;
    // /proc/stat's cpu fields are kernel-formatted decimal counters; a malformed
    // field would mean something is badly wrong with the running kernel, not a
    // condition worth failing the whole read over, so it degrades to 0 instead.
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .map(|field| field.parse::<u64>().unwrap_or(0))
        .collect();
    if fields.len() < 4 {
        return None;
    }
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    Some(CpuTimes {
        idle,
        total: fields.iter().take(GUEST_FREE_FIELDS).sum(),
    })
}

impl ProcStat for ProcStatFile {
    fn read(&mut self) -> Result<CpuTimes, String> {
        let text = read_trimmed(&self.path)?;
        parse_proc_stat(&text).ok_or_else(|| format!("{}: no cpu line", self.path.display()))
    }
}

pub struct CpuFreqSysfs {
    cpus: PathBuf,
    cpuinfo: PathBuf,
}

impl CpuFreqSysfs {
    pub fn new(root: &Path) -> Self {
        Self {
            cpus: root.join("sys/devices/system/cpu"),
            cpuinfo: root.join("proc/cpuinfo"),
        }
    }

    fn scaling_files_mhz(&self) -> Vec<f64> {
        let Ok(entries) = fs::read_dir(&self.cpus) else {
            return Vec::new();
        };
        let mut cores = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name
                .strip_prefix("cpu")
                .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
            {
                continue;
            }
            if let Ok(khz) = read_u64(&entry.path().join("cpufreq/scaling_cur_freq")) {
                cores.push(khz as f64 / 1000.0);
            }
        }
        cores
    }

    fn cpuinfo_mhz(&self) -> Vec<f64> {
        let Ok(text) = fs::read_to_string(&self.cpuinfo) else {
            return Vec::new();
        };
        parse_cpuinfo_mhz(&text)
    }
}

pub fn parse_cpuinfo_mhz(text: &str) -> Vec<f64> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| key.trim() == "cpu MHz")
        .filter_map(|(_, value)| value.trim().parse::<f64>().ok())
        .collect()
}

impl CpuFreq for CpuFreqSysfs {
    fn read_mhz(&mut self) -> Result<Vec<f64>, String> {
        let cores = self.scaling_files_mhz();
        if !cores.is_empty() {
            return Ok(cores);
        }
        let cores = self.cpuinfo_mhz();
        if cores.is_empty() {
            return Err("no cpufreq or cpuinfo frequencies".to_string());
        }
        Ok(cores)
    }
}

pub struct RaplSysfs {
    energy: PathBuf,
    modulus: Option<u64>,
}

impl RaplSysfs {
    pub fn discover(root: &Path) -> Option<Self> {
        for zone in RAPL_ZONES {
            let dir = root.join(zone);
            let energy = dir.join("energy_uj");
            if !is_package_zone(&dir) || read_u64(&energy).is_err() {
                continue;
            }
            return Some(Self {
                modulus: read_u64(&dir.join("max_energy_range_uj"))
                    .ok()
                    .and_then(|max| max.checked_add(1)),
                energy,
            });
        }
        amd_energy_input(root).map(|energy| Self {
            energy,
            modulus: None,
        })
    }
}

fn is_package_zone(dir: &Path) -> bool {
    read_trimmed(&dir.join("name")).is_ok_and(|name| name.starts_with(RAPL_PACKAGE_NAME_PREFIX))
}

fn amd_energy_input(root: &Path) -> Option<PathBuf> {
    let mut chips: Vec<PathBuf> = fs::read_dir(root.join("sys/class/hwmon"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    chips.sort();
    chips.into_iter().find_map(|chip| {
        let name = read_trimmed(&chip.join("name")).ok()?;
        if name != "amd_energy" {
            return None;
        }
        let energy = chip.join("energy1_input");
        read_u64(&energy).ok().map(|_| energy)
    })
}

impl Rapl for RaplSysfs {
    fn read_uj(&mut self) -> Result<u64, String> {
        read_u64(&self.energy)
    }

    fn energy_modulus_uj(&self) -> Option<u64> {
        self.modulus
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_hw::Backend;
    use std::time::{Duration, Instant};

    fn one_second_per_call() -> Box<dyn FnMut() -> Instant + Send> {
        let base = Instant::now();
        let mut seconds = 0;
        Box::new(move || {
            seconds += 1;
            base + Duration::from_secs(seconds)
        })
    }

    fn write(path: PathBuf, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root.join("proc/stat"),
            "cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 1 2 3 4 5 6 7 8 9 10\nintr 1 2 3\n",
        );
        write(
            root.join("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq"),
            "3000000\n",
        );
        write(
            root.join("sys/devices/system/cpu/cpu1/cpufreq/scaling_cur_freq"),
            "4000000\n",
        );
        write(root.join("sys/devices/system/cpu/cpufreq/boost"), "1\n");
        write(
            root.join("sys/class/powercap/intel-rapl:0/name"),
            "package-0\n",
        );
        write(
            root.join("sys/class/powercap/intel-rapl:0/energy_uj"),
            "1000000\n",
        );
        write(
            root.join("sys/class/powercap/intel-rapl:0/max_energy_range_uj"),
            "262143328850\n",
        );
        dir
    }

    #[test]
    fn proc_stat_folds_iowait_into_idle_and_sums_every_field() {
        let times = parse_proc_stat("cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 1 1 1 1\n").unwrap();
        assert_eq!(
            times,
            CpuTimes {
                idle: 800,
                total: 1000
            }
        );
        assert_eq!(parse_proc_stat("intr 1 2 3\n"), None);
        assert_eq!(parse_proc_stat("cpu 1 2\n"), None);
    }

    #[test]
    fn proc_stat_leaves_out_guest_time_the_kernel_already_folded_into_user_and_nice() {
        let times = parse_proc_stat("cpu  100 0 100 700 100 0 0 0 40 60\n").unwrap();
        assert_eq!(
            times,
            CpuTimes {
                idle: 800,
                total: 1000
            }
        );
    }

    #[test]
    fn cpufreq_reads_khz_from_every_core_directory() {
        let dir = fixture();
        let mut freq = CpuFreqSysfs::new(dir.path());
        let mut cores = freq.read_mhz().unwrap();
        cores.sort_by(f64::total_cmp);
        assert_eq!(cores, vec![3000.0, 4000.0]);
    }

    #[test]
    fn cpufreq_averages_the_cores_that_expose_a_scaling_file() {
        let dir = fixture();
        write(
            dir.path()
                .join("sys/devices/system/cpu/cpu2/cpufreq/scaling_max_freq"),
            "5000000\n",
        );
        let mut freq = CpuFreqSysfs::new(dir.path());
        let mut cores = freq.read_mhz().unwrap();
        cores.sort_by(f64::total_cmp);
        assert_eq!(cores, vec![3000.0, 4000.0]);
    }

    #[test]
    fn cpufreq_falls_back_to_cpuinfo_when_sysfs_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path().join("proc/cpuinfo"),
            "processor\t: 0\ncpu MHz\t\t: 3600.250\nprocessor\t: 1\ncpu MHz\t\t: 4400.750\n",
        );
        let mut freq = CpuFreqSysfs::new(dir.path());
        assert_eq!(freq.read_mhz().unwrap(), vec![3600.25, 4400.75]);

        let empty = tempfile::tempdir().unwrap();
        assert!(CpuFreqSysfs::new(empty.path()).read_mhz().is_err());
    }

    #[test]
    fn rapl_discovery_prefers_the_powercap_zone_and_keeps_its_wrap_range() {
        let dir = fixture();
        let mut rapl = RaplSysfs::discover(dir.path()).unwrap();
        assert_eq!(rapl.read_uj().unwrap(), 1_000_000);
        assert_eq!(rapl.energy_modulus_uj(), Some(262_143_328_851));
    }

    #[test]
    fn a_powercap_zone_that_is_not_a_package_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path().join("sys/class/powercap/intel-rapl:0/name"),
            "psys\n",
        );
        write(
            dir.path().join("sys/class/powercap/intel-rapl:0/energy_uj"),
            "1000000\n",
        );
        assert!(RaplSysfs::discover(dir.path()).is_none());
    }

    #[test]
    fn rapl_discovery_falls_back_to_the_mmio_zone_then_to_amd_energy() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path().join("sys/class/powercap/intel-rapl-mmio:0/name"),
            "package-0\n",
        );
        write(
            dir.path()
                .join("sys/class/powercap/intel-rapl-mmio:0/energy_uj"),
            "42\n",
        );
        assert_eq!(
            RaplSysfs::discover(dir.path()).unwrap().read_uj().unwrap(),
            42
        );

        let amd = tempfile::tempdir().unwrap();
        write(amd.path().join("sys/class/hwmon/hwmon0/name"), "k10temp\n");
        write(
            amd.path().join("sys/class/hwmon/hwmon1/name"),
            "amd_energy\n",
        );
        write(
            amd.path().join("sys/class/hwmon/hwmon1/energy1_input"),
            "77\n",
        );
        let mut rapl = RaplSysfs::discover(amd.path()).unwrap();
        assert_eq!(rapl.read_uj().unwrap(), 77);
        assert_eq!(rapl.energy_modulus_uj(), None);
    }

    #[test]
    fn a_machine_without_rapl_gets_usage_and_clock_only() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path().join("proc/stat"), "cpu 1 1 1 1\n");
        write(
            dir.path()
                .join("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq"),
            "2000000\n",
        );
        assert!(RaplSysfs::discover(dir.path()).is_none());
        let mut backend = crate::CpuBackend::new(sources_under(dir.path()));
        assert_eq!(
            backend
                .enumerate()
                .unwrap()
                .sensors
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            vec!["cpu/usage", "cpu/clock"]
        );
    }

    #[test]
    fn a_full_fixture_enumerates_all_three_sensors_and_reads_them() {
        let dir = fixture();
        let mut backend =
            crate::CpuBackend::new(sources_under(dir.path())).with_clock(one_second_per_call());
        let inventory = backend.enumerate().unwrap();
        assert_eq!(inventory.sensors.len(), 3);
        let first = backend.read_all();
        assert_eq!(first["cpu/usage"], None);
        assert_eq!(first["cpu/power"], None);
        assert_eq!(first["cpu/clock"], Some(3500.0));

        write(
            dir.path().join("proc/stat"),
            "cpu  200 0 100 800 100 0 0 0 0 0\n",
        );
        write(
            dir.path().join("sys/class/powercap/intel-rapl:0/energy_uj"),
            "2000000\n",
        );
        let second = backend.read_all();
        assert_eq!(second["cpu/usage"], Some(50.0));
        assert_eq!(second["cpu/power"], Some(0.5));
    }
}
