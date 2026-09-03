// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
// Copyright (C) LibreHardwareMonitor and Contributors.
// Ported to Rust for Gale from LibreHardwareMonitor
// LibreHardwareMonitorLib/Hardware/Motherboard/Lpc/Nct677X.cs
// at commit 8cbda900bb52a6a8f0cfe39d41aa4d48938e5554.

use std::collections::HashSet;

use crate::detect::{Chip, DetectedChip};
use crate::transport::PortIo;

pub const ADDRESS_REGISTER_OFFSET: u16 = 0x05;
pub const DATA_REGISTER_OFFSET: u16 = 0x06;
pub const BANK_SELECT_REGISTER: u8 = 0x4E;
pub const VENDOR_ID_HIGH_REGISTER: u16 = 0x804F;
pub const VENDOR_ID_LOW_REGISTER: u16 = 0x004F;
pub const NUVOTON_VENDOR_ID: u16 = 0x5CA3;

pub const FAN_CONTROL_MODE_REG: [u16; 7] = [0x102, 0x202, 0x302, 0x802, 0x902, 0xA02, 0xB02];
pub const FAN_PWM_COMMAND_REG: [u16; 7] = [0x109, 0x209, 0x309, 0x809, 0x909, 0xA09, 0xB09];
pub const FAN_PWM_OUT_REG_NCT6797_98_99: [u16; 7] =
    [0x001, 0x003, 0x011, 0x013, 0x015, 0xA09, 0xB09];
pub const FAN_PWM_OUT_REG_DEFAULT: [u16; 7] = [0x001, 0x003, 0x011, 0x013, 0x015, 0x017, 0x029];
pub const FAN_COUNT_REG: [u16; 7] = [0x4B0, 0x4B2, 0x4B4, 0x4B6, 0x4B8, 0x4BA, 0x4CC];
pub const FAN_RPM_REG_NCT6771_76: [u16; 5] = [0x656, 0x658, 0x65A, 0x65C, 0x65E];
pub const MAX_FAN_COUNT: u32 = 0x1FFF;
pub const MIN_FAN_COUNT: u32 = 0x15;
pub const FAN_COUNT_CLOCK: f64 = 1_350_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TempSource {
    pub label: &'static str,
    pub source: u8,
    pub register: u16,
    pub half_register: u16,
    pub half_bit: i8,
    pub source_register: u16,
    pub alternate_register: Option<u16>,
}

const fn temp(
    label: &'static str,
    source: u8,
    register: u16,
    half_register: u16,
    half_bit: i8,
    source_register: u16,
    alternate_register: Option<u16>,
) -> TempSource {
    TempSource {
        label,
        source,
        register,
        half_register,
        half_bit,
        source_register,
        alternate_register,
    }
}

pub const TEMPS_GROUP_D: &[TempSource] = &[
    temp("PECI_0", 16, 0x073, 0x074, 7, 0x100, None),
    temp("CPUTIN", 2, 0x075, 0x076, 7, 0x200, Some(0x491)),
    temp("SYSTIN", 1, 0x077, 0x078, 7, 0x300, Some(0x490)),
    temp("AUXTIN0", 3, 0x079, 0x07A, 7, 0x800, Some(0x492)),
    temp("AUXTIN1", 4, 0x07B, 0x07C, 7, 0x900, Some(0x493)),
    temp("AUXTIN2", 5, 0x07D, 0x07E, 7, 0xA00, Some(0x494)),
    temp("AUXTIN3", 6, 0x4A0, 0x49E, 6, 0xB00, Some(0x495)),
    temp("AUXTIN4", 7, 0x027, 0, -1, 0x621, None),
    temp("TSENSOR", 10, 0x4A2, 0x4A1, 7, 0xC00, Some(0x496)),
    temp("SMBUSMASTER0", 8, 0x150, 0x151, 7, 0x622, None),
    temp("SMBUSMASTER1", 9, 0x670, 0, -1, 0xC26, None),
    temp("PECI_1", 17, 0x672, 0, -1, 0xC27, None),
    temp(
        "PCH_CHIP_CPU_MAX_TEMP",
        18,
        0x674,
        0,
        -1,
        0xC28,
        Some(0x400),
    ),
    temp("PCH_CHIP_TEMP", 19, 0x676, 0, -1, 0xC29, Some(0x401)),
    temp("PCH_CPU_TEMP", 20, 0x678, 0, -1, 0xC2A, Some(0x402)),
    temp("PCH_MCH_TEMP", 21, 0x67A, 0, -1, 0xC2B, Some(0x404)),
    temp("AGENT0_DIMM0", 22, 0x405, 0, -1, 0, None),
    temp("AGENT0_DIMM1", 23, 0x406, 0, -1, 0, None),
    temp("AGENT1_DIMM0", 24, 0x407, 0, -1, 0, None),
    temp("AGENT1_DIMM1", 25, 0x408, 0, -1, 0, None),
    temp("BYTE_TEMP0", 26, 0x419, 0, -1, 0, None),
    temp("BYTE_TEMP1", 27, 0x41A, 0, -1, 0, None),
    temp("PECI_0_CAL", 28, 0x4F4, 0, -1, 0, None),
    temp("PECI_1_CAL", 29, 0x4F5, 0, -1, 0, None),
    temp("VIRTUAL_TEMP", 31, 0, 0, -1, 0, None),
    temp("SPARE_TEMP", 32, 0, 0, -1, 0, None),
    temp("SPARE_TEMP2", 33, 0, 0, -1, 0, None),
];

pub const TEMPS_GROUP_B: &[TempSource] = &[
    temp("PECI_0", 16, 0x073, 0x074, 7, 0x100, None),
    temp("CPUTIN", 2, 0x075, 0x076, 7, 0x200, Some(0x491)),
    temp("SYSTIN", 1, 0x077, 0x078, 7, 0x300, Some(0x490)),
    temp("AUXTIN0", 3, 0x079, 0x07A, 7, 0x800, Some(0x492)),
    temp("AUXTIN1", 4, 0x07B, 0x07C, 7, 0x900, Some(0x493)),
    temp("AUXTIN2", 5, 0x07D, 0x07E, 7, 0xA00, Some(0x494)),
    temp("AUXTIN3", 6, 0x4A0, 0x49E, 6, 0xB00, Some(0x495)),
    temp("AUXTIN4", 7, 0x027, 0, -1, 0x621, None),
    temp("SMBUSMASTER0", 8, 0x150, 0x151, 7, 0x622, None),
    temp("SMBUSMASTER1", 9, 0x670, 0, -1, 0xC26, None),
    temp("PECI_1", 17, 0x672, 0, -1, 0xC27, None),
    temp(
        "PCH_CHIP_CPU_MAX_TEMP",
        18,
        0x674,
        0,
        -1,
        0xC28,
        Some(0x400),
    ),
    temp("PCH_CHIP_TEMP", 19, 0x676, 0, -1, 0xC29, Some(0x401)),
    temp("PCH_CPU_TEMP", 20, 0x678, 0, -1, 0xC2A, Some(0x402)),
    temp("PCH_MCH_TEMP", 21, 0x67A, 0, -1, 0xC2B, Some(0x404)),
    temp("AGENT0_DIMM0", 22, 0x405, 0, -1, 0, None),
    temp("AGENT0_DIMM1", 23, 0x406, 0, -1, 0, None),
    temp("AGENT1_DIMM0", 24, 0x407, 0, -1, 0, None),
    temp("AGENT1_DIMM1", 25, 0x408, 0, -1, 0, None),
    temp("BYTE_TEMP0", 26, 0x419, 0, -1, 0, None),
    temp("BYTE_TEMP1", 27, 0x41A, 0, -1, 0, None),
    temp("PECI_0_CAL", 28, 0x4F4, 0, -1, 0, None),
    temp("PECI_1_CAL", 29, 0x4F5, 0, -1, 0, None),
    temp("VIRTUAL_TEMP", 31, 0, 0, -1, 0, None),
    temp("SPARE_TEMP", 32, 0, 0, -1, 0, None),
    temp("SPARE_TEMP2", 33, 0, 0, -1, 0, None),
];

pub const TEMPS_GROUP_A: &[TempSource] = &[
    temp("PECI_0", 16, 0x073, 0x074, 7, 0x100, None),
    temp("CPUTIN", 2, 0x075, 0x076, 7, 0x200, Some(0x491)),
    temp("SYSTIN", 1, 0x077, 0x078, 7, 0x300, Some(0x490)),
    temp("AUXTIN0", 3, 0x079, 0x07A, 7, 0x800, Some(0x492)),
    temp("AUXTIN1", 4, 0x07B, 0x07C, 7, 0x900, Some(0x493)),
    temp("AUXTIN2", 5, 0x07D, 0x07E, 7, 0xA00, Some(0x494)),
    temp("AUXTIN3", 6, 0x4A0, 0x49E, 6, 0xB00, Some(0x495)),
    temp("AUXTIN4", 7, 0x027, 0, -1, 0x621, None),
    temp("PECI_1", 17, 0x672, 0, -1, 0xC27, None),
    temp(
        "PCH_CHIP_CPU_MAX_TEMP",
        18,
        0x674,
        0,
        -1,
        0xC28,
        Some(0x400),
    ),
    temp("PCH_CHIP_TEMP", 19, 0x676, 0, -1, 0xC29, Some(0x401)),
    temp("PCH_CPU_TEMP", 20, 0x678, 0, -1, 0xC2A, Some(0x402)),
    temp("PCH_MCH_TEMP", 21, 0x67A, 0, -1, 0xC2B, Some(0x404)),
    temp("AGENT0_DIMM0", 22, 0x405, 0, -1, 0, None),
    temp("AGENT0_DIMM1", 23, 0x406, 0, -1, 0, None),
    temp("AGENT1_DIMM0", 24, 0x407, 0, -1, 0, None),
    temp("AGENT1_DIMM1", 25, 0x408, 0, -1, 0, None),
    temp("SMBUSMASTER0", 8, 0x150, 0x151, 7, 0x622, None),
    temp("SMBUSMASTER1", 9, 0x670, 0, -1, 0xC26, None),
    temp("BYTE_TEMP0", 26, 0x419, 0, -1, 0, None),
    temp("BYTE_TEMP1", 27, 0x41A, 0, -1, 0, None),
    temp("PECI_0_CAL", 28, 0x4F4, 0, -1, 0, None),
    temp("PECI_1_CAL", 29, 0x4F5, 0, -1, 0, None),
    temp("VIRTUAL_TEMP", 31, 0, 0, -1, 0, None),
    temp("SPARE_TEMP", 32, 0, 0, -1, 0, None),
    temp("SPARE_TEMP2", 33, 0, 0, -1, 0, None),
];

pub const TEMPS_DEFAULT: &[TempSource] = &[
    temp("PECI_0", 16, 0x027, 0, -1, 0x621, None),
    temp("CPUTIN", 2, 0x073, 0x074, 7, 0x100, Some(0x491)),
    temp("SYSTIN", 1, 0x075, 0x076, 7, 0x200, Some(0x490)),
    temp("AUXTIN0", 3, 0x077, 0x078, 7, 0x300, Some(0x492)),
    temp("AUXTIN1", 4, 0x079, 0x07A, 7, 0x800, Some(0x493)),
    temp("AUXTIN2", 5, 0x07B, 0x07C, 7, 0x900, Some(0x494)),
    temp("AUXTIN3", 6, 0x150, 0x151, 7, 0x622, Some(0x495)),
];

pub const TEMPS_NCT6771F: &[TempSource] = &[
    temp("PECI_0", 5, 0x027, 0, -1, 0x621, None),
    temp("CPUTIN", 2, 0x073, 0x074, 7, 0x100, None),
    temp("AUXTIN", 3, 0x075, 0x076, 7, 0x200, None),
    temp("SYSTIN", 1, 0x077, 0x078, 7, 0x300, None),
    temp("RESERVED", 0xFF, 0x150, 0x151, 7, 0x622, None),
    temp("RESERVED", 0xFF, 0x250, 0x251, 7, 0x623, None),
    temp("RESERVED", 0xFF, 0x62B, 0x62E, 0, 0x624, None),
    temp("RESERVED", 0xFF, 0x62C, 0x62E, 1, 0x625, None),
    temp("RESERVED", 0xFF, 0x62D, 0x62E, 2, 0x626, None),
];

pub const TEMPS_NCT6776F: &[TempSource] = &[
    temp("PECI_0", 12, 0x027, 0, -1, 0x621, None),
    temp("CPUTIN", 2, 0x073, 0x074, 7, 0x100, None),
    temp("AUXTIN", 3, 0x075, 0x076, 7, 0x200, None),
    temp("SYSTIN", 1, 0x077, 0x078, 7, 0x300, None),
    temp("RESERVED", 0xFF, 0x150, 0x151, 7, 0x622, None),
    temp("RESERVED", 0xFF, 0x250, 0x251, 7, 0x623, None),
    temp("RESERVED", 0xFF, 0x62B, 0x62E, 0, 0x624, None),
    temp("RESERVED", 0xFF, 0x62C, 0x62E, 1, 0x625, None),
    temp("RESERVED", 0xFF, 0x62D, 0x62E, 2, 0x626, None),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedControl {
    pub mode: u8,
    pub pwm: u8,
}

pub struct Nct677x {
    chip: Chip,
    base: u16,
    fan_count: usize,
    control_count: usize,
    exposed_temps: usize,
    temps: &'static [TempSource],
    pwm_out: [u16; 7],
    saved: [Option<SavedControl>; 7],
}

fn fan_count_for(chip: Chip) -> usize {
    match chip {
        Chip::Nct6771F => 4,
        Chip::Nct6776F => 5,
        Chip::Nct6779D => 5,
        Chip::Nct6791D
        | Chip::Nct6792D
        | Chip::Nct6792DA
        | Chip::Nct6793D
        | Chip::Nct6795D
        | Chip::Nct6796D => 6,
        Chip::Nct6796DR | Chip::Nct6797D | Chip::Nct6798D | Chip::Nct6799D => 7,
    }
}

fn control_count_for(chip: Chip) -> usize {
    match chip {
        Chip::Nct6771F => 3,
        Chip::Nct6776F => 3,
        Chip::Nct6779D => 5,
        Chip::Nct6791D
        | Chip::Nct6792D
        | Chip::Nct6792DA
        | Chip::Nct6793D
        | Chip::Nct6795D
        | Chip::Nct6796D => 6,
        Chip::Nct6796DR | Chip::Nct6797D | Chip::Nct6798D | Chip::Nct6799D => 7,
    }
}

fn temps_for(chip: Chip) -> &'static [TempSource] {
    match chip {
        Chip::Nct6798D | Chip::Nct6799D => TEMPS_GROUP_D,
        Chip::Nct6796D | Chip::Nct6796DR | Chip::Nct6797D => TEMPS_GROUP_B,
        Chip::Nct6791D | Chip::Nct6792D | Chip::Nct6793D | Chip::Nct6795D => TEMPS_GROUP_A,
        Chip::Nct6779D | Chip::Nct6792DA => TEMPS_DEFAULT,
        Chip::Nct6771F => TEMPS_NCT6771F,
        Chip::Nct6776F => TEMPS_NCT6776F,
    }
}

fn exposed_temps_for(chip: Chip) -> usize {
    match chip {
        Chip::Nct6771F | Chip::Nct6776F => 4,
        other => temps_for(other).len(),
    }
}

fn pwm_out_for(chip: Chip) -> [u16; 7] {
    match chip {
        Chip::Nct6797D | Chip::Nct6798D | Chip::Nct6799D => FAN_PWM_OUT_REG_NCT6797_98_99,
        _ => FAN_PWM_OUT_REG_DEFAULT,
    }
}

fn min_rpm_for(chip: Chip) -> Option<u32> {
    match chip {
        Chip::Nct6771F => Some(20),
        Chip::Nct6776F => Some(164),
        _ => None,
    }
}

fn warn_once(warned: &mut bool) {
    if !*warned {
        tracing::warn!("super i/o register read failed");
        *warned = true;
    }
}

impl Nct677x {
    pub fn new(io: &mut dyn PortIo, detected: &DetectedChip) -> Result<Self, String> {
        let instance = Self {
            chip: detected.chip,
            base: detected.base,
            fan_count: fan_count_for(detected.chip),
            control_count: control_count_for(detected.chip),
            exposed_temps: exposed_temps_for(detected.chip),
            temps: temps_for(detected.chip),
            pwm_out: pwm_out_for(detected.chip),
            saved: [None; 7],
        };
        let vendor = instance.read_vendor_id(io)?;
        if vendor != NUVOTON_VENDOR_ID {
            return Err(format!("vendor id 0x{vendor:04x} is not Nuvoton"));
        }
        Ok(instance)
    }

    pub fn chip(&self) -> Chip {
        self.chip
    }

    pub fn base(&self) -> u16 {
        self.base
    }

    pub fn fan_count(&self) -> usize {
        self.fan_count
    }

    pub fn control_count(&self) -> usize {
        self.control_count
    }

    pub fn temperature_channels(&self) -> Vec<(usize, &'static str)> {
        self.temps
            .iter()
            .enumerate()
            .take(self.exposed_temps)
            .filter(|(_, entry)| entry.register != 0 || entry.alternate_register.is_some())
            .map(|(index, entry)| (index + 1, entry.label))
            .collect()
    }

    pub fn read_byte(&self, io: &mut dyn PortIo, address: u16) -> Result<u8, String> {
        io.pio_outb(self.base + ADDRESS_REGISTER_OFFSET, BANK_SELECT_REGISTER)?;
        io.pio_outb(self.base + DATA_REGISTER_OFFSET, (address >> 8) as u8)?;
        io.pio_outb(self.base + ADDRESS_REGISTER_OFFSET, (address & 0xFF) as u8)?;
        io.pio_inb(self.base + DATA_REGISTER_OFFSET)
    }

    pub fn write_byte(&self, io: &mut dyn PortIo, address: u16, value: u8) -> Result<(), String> {
        io.pio_outb(self.base + ADDRESS_REGISTER_OFFSET, BANK_SELECT_REGISTER)?;
        io.pio_outb(self.base + DATA_REGISTER_OFFSET, (address >> 8) as u8)?;
        io.pio_outb(self.base + ADDRESS_REGISTER_OFFSET, (address & 0xFF) as u8)?;
        io.pio_outb(self.base + DATA_REGISTER_OFFSET, value)
    }

    fn read_vendor_id(&self, io: &mut dyn PortIo) -> Result<u16, String> {
        let high = self.read_byte(io, VENDOR_ID_HIGH_REGISTER)?;
        let low = self.read_byte(io, VENDOR_ID_LOW_REGISTER)?;
        Ok(((high as u16) << 8) | low as u16)
    }

    pub fn vendor_ok(&self, io: &mut dyn PortIo) -> Result<bool, String> {
        Ok(self.read_vendor_id(io)? == NUVOTON_VENDOR_ID)
    }

    pub fn read_fans(&self, io: &mut dyn PortIo) -> Vec<Option<f64>> {
        let mut warned = false;
        if let Some(min_rpm) = min_rpm_for(self.chip) {
            FAN_RPM_REG_NCT6771_76
                .iter()
                .take(self.fan_count)
                .map(|&reg| {
                    let high = match self.read_byte(io, reg) {
                        Ok(value) => value,
                        Err(_) => {
                            warn_once(&mut warned);
                            return None;
                        }
                    };
                    let low = match self.read_byte(io, reg + 1) {
                        Ok(value) => value,
                        Err(_) => {
                            warn_once(&mut warned);
                            return None;
                        }
                    };
                    let rpm = ((high as u32) << 8) | low as u32;
                    if rpm <= min_rpm {
                        Some(0.0)
                    } else {
                        Some(rpm as f64)
                    }
                })
                .collect()
        } else {
            FAN_COUNT_REG
                .iter()
                .take(self.fan_count)
                .map(|&reg| {
                    let high = match self.read_byte(io, reg) {
                        Ok(value) => value,
                        Err(_) => {
                            warn_once(&mut warned);
                            return None;
                        }
                    };
                    let low = match self.read_byte(io, reg + 1) {
                        Ok(value) => value,
                        Err(_) => {
                            warn_once(&mut warned);
                            return None;
                        }
                    };
                    fan_from_count(high, low)
                })
                .collect()
        }
    }

    fn compute_all_temperatures(&self, io: &mut dyn PortIo) -> Vec<Option<f64>> {
        let mut warned = false;
        let mut values: Vec<Option<f64>> = vec![None; self.temps.len()];
        let mut accepted: HashSet<u8> = HashSet::new();

        for entry in self.temps.iter() {
            if entry.register == 0 {
                continue;
            }
            let raw = match self.read_byte(io, entry.register) {
                Ok(value) => value,
                Err(_) => {
                    warn_once(&mut warned);
                    continue;
                }
            };
            let mut value = (raw as i8 as i32) << 1;
            if entry.half_bit > 0 {
                let half = match self.read_byte(io, entry.half_register) {
                    Ok(value) => value,
                    Err(_) => {
                        warn_once(&mut warned);
                        continue;
                    }
                };
                value |= ((half >> entry.half_bit) & 1) as i32;
            }
            let source = if entry.source_register != 0 {
                match self.read_byte(io, entry.source_register) {
                    Ok(value) => value & 0x1F,
                    Err(_) => {
                        warn_once(&mut warned);
                        continue;
                    }
                }
            } else {
                entry.source
            };
            if accepted.contains(&source) {
                continue;
            }
            let reading = 0.5 * value as f64;
            if !(-55.0..=125.0).contains(&reading) {
                continue;
            }
            accepted.insert(source);
            for (index, other) in self.temps.iter().enumerate() {
                if other.source == source {
                    values[index] = Some(reading);
                }
            }
        }

        for (index, entry) in self.temps.iter().enumerate() {
            let Some(alternate) = entry.alternate_register else {
                continue;
            };
            if accepted.contains(&entry.source) {
                continue;
            }
            let raw = match self.read_byte(io, alternate) {
                Ok(value) => value,
                Err(_) => {
                    warn_once(&mut warned);
                    continue;
                }
            };
            let reading = raw as i8 as f64;
            if reading > 125.0 || reading <= 0.0 {
                continue;
            }
            values[index] = Some(reading);
        }

        values
    }

    pub fn read_temperatures(&self, io: &mut dyn PortIo) -> Vec<Option<f64>> {
        let values = self.compute_all_temperatures(io);
        self.temps
            .iter()
            .enumerate()
            .take(self.exposed_temps)
            .filter(|(_, entry)| entry.register != 0 || entry.alternate_register.is_some())
            .map(|(index, _)| values[index])
            .collect()
    }

    pub fn read_controls(&self, io: &mut dyn PortIo) -> Vec<Option<f64>> {
        let mut warned = false;
        self.pwm_out
            .iter()
            .take(self.control_count)
            .map(|&reg| match self.read_byte(io, reg) {
                Ok(value) => Some(raw_to_duty(value)),
                Err(_) => {
                    warn_once(&mut warned);
                    None
                }
            })
            .collect()
    }

    pub fn saved(&self, index: usize) -> Option<SavedControl> {
        self.saved.get(index).copied().flatten()
    }
}

pub fn duty_to_raw(pct: f64) -> u8 {
    (pct.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8
}

pub fn raw_to_duty(raw: u8) -> f64 {
    raw as f64 / 2.55
}

pub fn fan_from_count(high: u8, low: u8) -> Option<f64> {
    let count = ((high as u32) << 5) | (low as u32 & 0x1F);
    if count >= MAX_FAN_COUNT {
        Some(0.0)
    } else if count >= MIN_FAN_COUNT {
        Some(FAN_COUNT_CLOCK / count as f64)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::{self, Chip, DetectedChip};
    use crate::transport::{Call, FakeChip, FakePortIo};

    fn nct6798d(io: &mut FakePortIo) -> Nct677x {
        detect::reselect(io, 0).unwrap();
        let detected = DetectedChip {
            chip: Chip::Nct6798D,
            slot: 0,
            revision: 0x2B,
            base: 0x0290,
        };
        Nct677x::new(io, &detected).unwrap()
    }

    #[test]
    fn new_checks_vendor_id_with_exact_call_sequence() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        detect::reselect(&mut io, 0).unwrap();
        let detected = DetectedChip {
            chip: Chip::Nct6798D,
            slot: 0,
            revision: 0x2B,
            base: 0x0290,
        };
        io.take_calls();
        Nct677x::new(&mut io, &detected).unwrap();
        assert_eq!(
            io.take_calls(),
            vec![
                Call::PioOut(0x295, 0x4E),
                Call::PioOut(0x296, 0x80),
                Call::PioOut(0x295, 0x4F),
                Call::PioIn(0x296),
                Call::PioOut(0x295, 0x4E),
                Call::PioOut(0x296, 0x00),
                Call::PioOut(0x295, 0x4F),
                Call::PioIn(0x296),
            ]
        );
    }

    #[test]
    fn new_rejects_a_non_nuvoton_vendor_id() {
        let mut chip = FakeChip::nct6798d(0x0290);
        chip.hm.insert(0x804F, 0x12);
        let mut io = FakePortIo::with_chip(0, chip);
        detect::reselect(&mut io, 0).unwrap();
        let detected = DetectedChip {
            chip: Chip::Nct6798D,
            slot: 0,
            revision: 0x2B,
            base: 0x0290,
        };
        match Nct677x::new(&mut io, &detected) {
            Err(err) => assert!(err.contains("0x12a3")),
            Ok(_) => panic!("expected a vendor id mismatch error"),
        }
    }

    #[test]
    fn read_byte_records_the_bank_select_sequence() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let chip = nct6798d(&mut io);
        io.take_calls();
        assert_eq!(chip.read_byte(&mut io, 0x4B0).unwrap(), 0);
        assert_eq!(
            io.take_calls(),
            vec![
                Call::PioOut(0x295, 0x4E),
                Call::PioOut(0x296, 0x04),
                Call::PioOut(0x295, 0xB0),
                Call::PioIn(0x296),
            ]
        );
    }

    #[test]
    fn write_byte_records_the_bank_select_sequence_and_updates_the_map() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let chip = nct6798d(&mut io);
        io.take_calls();
        chip.write_byte(&mut io, 0x102, 0x02).unwrap();
        assert_eq!(
            io.take_calls(),
            vec![
                Call::PioOut(0x295, 0x4E),
                Call::PioOut(0x296, 0x01),
                Call::PioOut(0x295, 0x02),
                Call::PioOut(0x296, 0x02),
            ]
        );
        assert_eq!(io.hm(0, 0x102), 0x02);
    }

    #[test]
    fn fan_from_count_examples() {
        assert_eq!(fan_from_count(0x23, 0x05), Some(1200.0));
        assert_eq!(fan_from_count(0x2A, 0x06), Some(1000.0));
        assert_eq!(fan_from_count(0xFF, 0x1F), Some(0.0));
        assert_eq!(fan_from_count(0x00, 0x10), None);
        assert_eq!(fan_from_count(0x00, 0x00), None);
    }

    #[test]
    fn read_fans_reports_all_seven_channels_for_nct6798d() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let chip = nct6798d(&mut io);
        io.set_hm(0, 0x4B0, 0x23);
        io.set_hm(0, 0x4B1, 0x05);
        io.set_hm(0, 0x4B2, 0x2A);
        io.set_hm(0, 0x4B3, 0x06);
        io.set_hm(0, 0x4B4, 0xFF);
        io.set_hm(0, 0x4B5, 0x1F);
        io.set_hm(0, 0x4B6, 0x00);
        io.set_hm(0, 0x4B7, 0x10);
        io.take_calls();
        assert_eq!(
            chip.read_fans(&mut io),
            vec![
                Some(1200.0),
                Some(1000.0),
                Some(0.0),
                None,
                None,
                None,
                None,
            ]
        );
    }

    #[test]
    fn read_fans_uses_16_bit_rpm_registers_on_nct6776f() {
        let mut fake_chip = FakeChip::nct6798d(0x0290);
        fake_chip.id = 0xC3;
        fake_chip.revision = 0x33;
        let mut io = FakePortIo::with_chip(0, fake_chip);
        detect::reselect(&mut io, 0).unwrap();
        let detected = DetectedChip {
            chip: Chip::Nct6776F,
            slot: 0,
            revision: 0x33,
            base: 0x0290,
        };
        let chip = Nct677x::new(&mut io, &detected).unwrap();
        io.set_hm(0, 0x656, 0x04);
        io.set_hm(0, 0x657, 0xB0);
        io.set_hm(0, 0x658, 0x00);
        io.set_hm(0, 0x659, 0x50);
        io.take_calls();

        assert_eq!(
            chip.read_fans(&mut io),
            vec![Some(1200.0), Some(0.0), Some(0.0), Some(0.0), Some(0.0)]
        );

        let calls = io.take_calls();
        let addresses: Vec<u16> = calls
            .chunks(4)
            .filter_map(|chunk| match chunk {
                [Call::PioOut(_, 0x4E), Call::PioOut(_, bank), Call::PioOut(_, low), Call::PioIn(_)] => {
                    Some(((*bank as u16) << 8) | *low as u16)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            addresses,
            vec![0x656, 0x657, 0x658, 0x659, 0x65A, 0x65B, 0x65C, 0x65D, 0x65E, 0x65F,]
        );
        assert!(!addresses.contains(&0x4B0));
    }

    #[test]
    fn read_controls_reads_pwm_out_registers_in_order() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let chip = nct6798d(&mut io);
        io.set_hm(0, 0x001, 0x80);
        io.set_hm(0, 0x003, 0xFF);
        io.take_calls();
        assert_eq!(
            chip.read_controls(&mut io),
            vec![
                Some(128.0 / 2.55),
                Some(100.0),
                Some(0.0),
                Some(0.0),
                Some(0.0),
                Some(0.0),
                Some(0.0),
            ]
        );
        let calls = io.take_calls();
        let bank_register_pairs: Vec<(u8, u8)> = calls
            .windows(3)
            .filter_map(|window| match (&window[0], &window[1], &window[2]) {
                (Call::PioOut(_, 0x4E), Call::PioOut(_, bank), Call::PioOut(_, register)) => {
                    Some((*bank, *register))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            bank_register_pairs,
            vec![
                (0x00, 0x01),
                (0x00, 0x03),
                (0x00, 0x11),
                (0x00, 0x13),
                (0x00, 0x15),
                (0x0A, 0x09),
                (0x0B, 0x09),
            ]
        );
    }

    #[test]
    fn duty_to_raw_examples() {
        assert_eq!(duty_to_raw(50.0), 128);
        assert_eq!(duty_to_raw(60.0), 153);
        assert_eq!(duty_to_raw(40.0), 102);
        assert_eq!(duty_to_raw(100.0), 255);
        assert_eq!(duty_to_raw(0.0), 0);
        assert_eq!(duty_to_raw(150.0), 255);
        assert_eq!(duty_to_raw(-5.0), 0);
    }

    #[test]
    fn read_temperatures_applies_the_source_dedup_and_alternate_algorithm() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let chip = nct6798d(&mut io);
        io.set_hm(0, 0x100, 0x10);
        io.set_hm(0, 0x073, 0x3C);
        io.set_hm(0, 0x074, 0x80);
        io.set_hm(0, 0x200, 0x02);
        io.set_hm(0, 0x075, 0x2D);
        io.set_hm(0, 0x076, 0x00);
        io.set_hm(0, 0x300, 0x01);
        io.set_hm(0, 0x077, 0x23);
        io.set_hm(0, 0x078, 0x80);
        io.set_hm(0, 0x800, 0x03);
        io.set_hm(0, 0x079, 0x80);
        io.set_hm(0, 0x492, 0x00);
        io.set_hm(0, 0x900, 0x04);
        io.set_hm(0, 0x07B, 0x80);
        io.set_hm(0, 0x493, 0x1E);
        io.set_hm(0, 0xA00, 0x02);
        io.set_hm(0, 0x07D, 0x50);
        io.set_hm(0, 0x494, 0x00);
        io.take_calls();

        let readings = chip.read_temperatures(&mut io);
        assert_eq!(
            &readings[0..6],
            &[Some(60.5), Some(45.0), Some(35.5), None, Some(30.0), None,]
        );

        let calls = io.take_calls();
        let cputin_alternate_read = calls.windows(2).any(|pair| {
            matches!(
                (&pair[0], &pair[1]),
                (Call::PioOut(0x296, 0x04), Call::PioOut(0x295, 0x91))
            )
        });
        assert!(!cputin_alternate_read);
    }

    #[test]
    fn temperature_channels_matches_the_nct6798d_and_nct6771f_reference_tables() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let chip = nct6798d(&mut io);
        let channels = chip.temperature_channels();
        assert_eq!(channels.len(), 24);
        let labels: Vec<&str> = channels.iter().map(|(_, label)| *label).collect();
        assert_eq!(
            &labels[..10],
            &[
                "PECI_0",
                "CPUTIN",
                "SYSTIN",
                "AUXTIN0",
                "AUXTIN1",
                "AUXTIN2",
                "AUXTIN3",
                "AUXTIN4",
                "TSENSOR",
                "SMBUSMASTER0",
            ]
        );

        let mut io = FakePortIo::with_chip(0, FakeChip::nct6779d(0x0A30));
        detect::reselect(&mut io, 0).unwrap();
        let detected = DetectedChip {
            chip: Chip::Nct6771F,
            slot: 0,
            revision: 0x71,
            base: 0x0A30,
        };
        let chip = Nct677x::new(&mut io, &detected).unwrap();
        assert_eq!(chip.temperature_channels().len(), 4);
    }

    #[test]
    fn table_shapes_match_the_reference_data() {
        assert_eq!(TEMPS_GROUP_D.len(), 27);
        assert_eq!(TEMPS_GROUP_B.len(), 26);
        assert!(!TEMPS_GROUP_B.iter().any(|entry| entry.label == "TSENSOR"));
        assert_eq!(TEMPS_GROUP_A[8].label, "PECI_1");
        assert_eq!(TEMPS_DEFAULT[0].register, 0x027);
        assert_eq!(TEMPS_NCT6771F[0].source, 5);
        assert_eq!(TEMPS_NCT6776F[0].source, 12);
    }

    #[test]
    fn channel_counts_match_the_reference_table() {
        assert_eq!(fan_count_for(Chip::Nct6771F), 4);
        assert_eq!(control_count_for(Chip::Nct6771F), 3);
        assert_eq!(fan_count_for(Chip::Nct6776F), 5);
        assert_eq!(control_count_for(Chip::Nct6776F), 3);
        assert_eq!(fan_count_for(Chip::Nct6779D), 5);
        assert_eq!(control_count_for(Chip::Nct6779D), 5);
        for chip in [
            Chip::Nct6791D,
            Chip::Nct6792D,
            Chip::Nct6792DA,
            Chip::Nct6793D,
            Chip::Nct6795D,
            Chip::Nct6796D,
        ] {
            assert_eq!(fan_count_for(chip), 6);
            assert_eq!(control_count_for(chip), 6);
        }
        for chip in [
            Chip::Nct6796DR,
            Chip::Nct6797D,
            Chip::Nct6798D,
            Chip::Nct6799D,
        ] {
            assert_eq!(fan_count_for(chip), 7);
            assert_eq!(control_count_for(chip), 7);
        }
    }

    #[test]
    fn pwm_out_table_selection_matches_the_reference_data() {
        assert_eq!(pwm_out_for(Chip::Nct6796D), FAN_PWM_OUT_REG_DEFAULT);
        assert_eq!(pwm_out_for(Chip::Nct6798D), FAN_PWM_OUT_REG_NCT6797_98_99);
    }

    #[test]
    fn fail_next_produces_none_for_the_failing_channel_only() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let chip = nct6798d(&mut io);
        io.set_hm(0, 0x4B2, 0x2A);
        io.set_hm(0, 0x4B3, 0x06);
        io.fail_next = Some("boom".to_string());
        let readings = chip.read_fans(&mut io);
        assert_eq!(readings[0], None);
        assert_eq!(readings[1], Some(1000.0));
    }
}
