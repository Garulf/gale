//! Ids that match what the Windows backends report for the same hardware, so a
//! profile written on one OS drives the same sensors and fans on the other.

use gale_hw::{Id, SensorKind};
use gale_hw_superio::detect::Chip;
use gale_hw_superio::nct677x::temperature_channels_of;
use std::fs;
use std::path::Path;

const SPD5118_FIRST_ADDRESS: u16 = 0x50;
const JC42_FIRST_ADDRESS: u16 = 0x18;
const DIMM_SLOTS: u16 = 8;

pub fn sensor_id(
    chip: &str,
    dir: &Path,
    stem: &str,
    label: Option<&str>,
    kind: SensorKind,
) -> Option<Id> {
    match chip {
        "k10temp" => amd_cpu_id(label?),
        "asusec" => asus_ec_id(label?, kind),
        "spd5118" if stem == "temp1" => dimm_id(dir, SPD5118_FIRST_ADDRESS),
        "jc42" if stem == "temp1" => dimm_id(dir, JC42_FIRST_ADDRESS),
        _ => {
            let superio = superio_chip(chip)?;
            superio_sensor_id(superio, stem, label)
        }
    }
}

pub fn control_id(chip: &str, file: &str) -> Option<Id> {
    let superio = superio_chip(chip)?;
    Some(format!("superio/{}/{file}", superio.slug()))
}

fn amd_cpu_id(label: &str) -> Option<Id> {
    let name = match label {
        "Tctl" => "tctl".to_string(),
        "Tdie" => "tdie".to_string(),
        other => format!("ccd{}", numbered(other, "Tccd")?),
    };
    Some(format!("cpu/amd/{name}"))
}

fn asus_ec_id(label: &str, kind: SensorKind) -> Option<Id> {
    let slug = match (kind, label) {
        (SensorKind::Temp, "Chipset") => "chipset",
        (SensorKind::Temp, "CPU") => "cpu",
        (SensorKind::Temp, "CPU Package") => "cpu_package",
        (SensorKind::Temp, "Motherboard") => "motherboard",
        (SensorKind::Temp, "T_Sensor") => "t_sensor",
        (SensorKind::Temp, "T_sensor_2" | "T_Sensor_2") => "t_sensor_2",
        (SensorKind::Temp, "VRM") => "vrm",
        (SensorKind::Temp, "Water_In") => "water_in",
        (SensorKind::Temp, "Water_Out") => "water_out",
        (SensorKind::Temp, "Water_Block_In") => "water_block_in",
        (SensorKind::Rpm, "CPU_Opt") => "cpu_optional_fan",
        (SensorKind::Rpm, "VRM HS") => "vrm_heat_sink_fan",
        (SensorKind::Rpm, "Chipset") => "chipset_fan",
        (SensorKind::Rpm, "Water_Flow") => "water_pump",
        _ => return None,
    };
    Some(format!("ec/asus/{slug}"))
}

fn dimm_id(dir: &Path, first_address: u16) -> Option<Id> {
    let device = fs::canonicalize(dir.join("device")).ok()?;
    let client = device.file_name()?.to_str()?;
    let (_, address) = client.split_once('-')?;
    let offset = u16::from_str_radix(address, 16)
        .ok()?
        .checked_sub(first_address)?;
    if offset >= DIMM_SLOTS {
        return None;
    }
    let slot = offset + 1;
    let adapter = fs::read_to_string(device.parent()?.join("name")).unwrap_or_default();
    match smbus_port(&adapter) {
        0 => Some(format!("dimm/{slot}/temp")),
        port => Some(format!("dimm/{slot}-bus{port}/temp")),
    }
}

fn smbus_port(adapter_name: &str) -> u32 {
    adapter_name
        .split_once("port ")
        .and_then(|(_, rest)| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
        .unwrap_or(0)
}

fn superio_chip(kernel_name: &str) -> Option<Chip> {
    Some(match kernel_name {
        "nct6775" => Chip::Nct6771F,
        "nct6776" => Chip::Nct6776F,
        "nct6779" => Chip::Nct6779D,
        "nct6791" => Chip::Nct6791D,
        "nct6792" => Chip::Nct6792D,
        "nct6793" => Chip::Nct6793D,
        "nct6795" => Chip::Nct6795D,
        "nct6796" => Chip::Nct6796D,
        "nct6797" => Chip::Nct6797D,
        "nct6798" => Chip::Nct6798D,
        "nct6799" => Chip::Nct6799D,
        _ => return None,
    })
}

fn superio_sensor_id(chip: Chip, stem: &str, label: Option<&str>) -> Option<Id> {
    let slug = chip.slug();
    if stem.starts_with("fan") {
        return Some(format!("superio/{slug}/{stem}"));
    }
    let wanted = superio_temp_label(label?);
    let (channel, _) = temperature_channels_of(chip)
        .into_iter()
        .find(|(_, label)| *label == wanted)?;
    Some(format!("superio/{slug}/temp{channel}"))
}

fn superio_temp_label(kernel_label: &str) -> String {
    if let Some(agent) = kernel_label.strip_prefix("PECI Agent ") {
        return match agent.strip_suffix(" Calibration") {
            Some(index) => format!("PECI_{index}_CAL"),
            None => format!("PECI_{agent}"),
        };
    }
    if let Some(index) = kernel_label.strip_prefix("SMBUSMASTER ") {
        return format!("SMBUSMASTER{index}");
    }
    kernel_label.to_ascii_uppercase().replace(' ', "_")
}

fn numbered(text: &str, prefix: &str) -> Option<u32> {
    text.strip_prefix(prefix)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_superio_labels_become_the_windows_labels() {
        assert_eq!(superio_temp_label("PECI Agent 0"), "PECI_0");
        assert_eq!(superio_temp_label("PECI Agent 1 Calibration"), "PECI_1_CAL");
        assert_eq!(superio_temp_label("SMBUSMASTER 0"), "SMBUSMASTER0");
        assert_eq!(superio_temp_label("Agent0 Dimm1"), "AGENT0_DIMM1");
        assert_eq!(superio_temp_label("Virtual_TEMP"), "VIRTUAL_TEMP");
        assert_eq!(superio_temp_label("SYSTIN"), "SYSTIN");
    }

    #[test]
    fn smbus_port_comes_from_the_piix4_adapter_name() {
        assert_eq!(smbus_port("SMBus PIIX4 adapter port 0 at 0b00"), 0);
        assert_eq!(smbus_port("SMBus PIIX4 adapter port 2 at 0b00"), 2);
        assert_eq!(smbus_port("SMBus I801 adapter at efa0"), 0);
    }

    #[test]
    fn k10temp_labels_map_to_amd_cpu_ids() {
        assert_eq!(amd_cpu_id("Tctl").as_deref(), Some("cpu/amd/tctl"));
        assert_eq!(amd_cpu_id("Tdie").as_deref(), Some("cpu/amd/tdie"));
        assert_eq!(amd_cpu_id("Tccd2").as_deref(), Some("cpu/amd/ccd2"));
        assert_eq!(amd_cpu_id("Tsomething"), None);
    }

    #[test]
    fn asus_ec_fans_and_temps_with_the_same_label_stay_distinct() {
        assert_eq!(
            asus_ec_id("Chipset", SensorKind::Temp).as_deref(),
            Some("ec/asus/chipset")
        );
        assert_eq!(
            asus_ec_id("Chipset", SensorKind::Rpm).as_deref(),
            Some("ec/asus/chipset_fan")
        );
    }
}
