// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from LibreHardwareMonitor, LibreHardwareMonitorLib/Hardware/Cpu/Amd17Cpu.cs.

pub const THM_TCON_CUR_TMP: u32 = 0x0005_9800;
pub const CCD1_TEMP_M70H: u32 = 0x0005_9954;
pub const CCD1_TEMP_M61H: u32 = 0x0005_9B08;
pub const MAX_CCDS: u32 = 8;
const TEMP_RANGE_SEL_MASK: u32 = 0x8_0000;
const TEMP_TJ_SEL_MASK: u32 = 0x3_0000;

pub fn tctl_from_raw(raw: u32) -> f64 {
    let offset_flag = raw & TEMP_RANGE_SEL_MASK != 0 || raw & TEMP_TJ_SEL_MASK == TEMP_TJ_SEL_MASK;
    let temperature = f64::from(raw >> 21) * 0.125;
    if offset_flag {
        temperature - 49.0
    } else {
        temperature
    }
}

pub fn brand_tctl_offset(brand: &str) -> f64 {
    if brand.contains("1600X") || brand.contains("1700X") || brand.contains("1800X") {
        -20.0
    } else if brand.contains("Threadripper 19") || brand.contains("Threadripper 29") {
        -27.0
    } else if brand.contains("2700X") {
        -10.0
    } else {
        0.0
    }
}

pub fn supports_per_ccd(model: u32) -> bool {
    matches!(model, 0x31 | 0x71 | 0x21 | 0x61 | 0x44)
}

pub fn ccd_register(model: u32, ccd: u32) -> u32 {
    let base = if matches!(model, 0x61 | 0x44) {
        CCD1_TEMP_M61H
    } else {
        CCD1_TEMP_M70H
    };
    base + ccd * 4
}

pub fn ccd_temp_from_raw(raw: u32) -> Option<f64> {
    let raw = raw & 0xFFF;
    let temperature = (f64::from(raw) * 125.0 - 305_000.0) * 0.001;
    (raw > 0 && temperature < 125.0).then_some(temperature)
}

pub const MSR_RAPL_PWR_UNIT: u32 = 0xC001_0299;
pub const MSR_PKG_ENERGY_STAT: u32 = 0xC001_029B;
pub const ENERGY_COUNTER_MODULUS: u64 = 1u64 << 32;
const ENERGY_UNIT_EXPONENTS: std::ops::RangeInclusive<u64> = 1..=31;

pub fn energy_unit_joules(raw: u64) -> Option<f64> {
    let exponent = (raw >> 8) & 0x1F;
    ENERGY_UNIT_EXPONENTS
        .contains(&exponent)
        .then(|| 1.0 / (1u64 << exponent) as f64)
}

pub fn package_energy_counter(raw: u64) -> u64 {
    raw & u64::from(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tctl_uses_the_top_eleven_bits_in_eighths_of_a_degree() {
        assert_eq!(tctl_from_raw(400 << 21), 50.0);
        assert_eq!(tctl_from_raw(401 << 21), 50.125);
    }

    #[test]
    fn tctl_subtracts_49_when_range_select_or_tj_select_says_so() {
        assert_eq!(tctl_from_raw((800 << 21) | TEMP_RANGE_SEL_MASK), 51.0);
        assert_eq!(tctl_from_raw((800 << 21) | TEMP_TJ_SEL_MASK), 51.0);
        assert_eq!(tctl_from_raw((800 << 21) | 0x1_0000), 100.0);
    }

    #[test]
    fn brand_offsets_follow_the_k10temp_table() {
        assert_eq!(
            brand_tctl_offset("AMD Ryzen 7 1800X Eight-Core Processor"),
            -20.0
        );
        assert_eq!(brand_tctl_offset("AMD Ryzen Threadripper 2950X"), -27.0);
        assert_eq!(brand_tctl_offset("AMD Ryzen 7 2700X"), -10.0);
        assert_eq!(
            brand_tctl_offset("AMD Ryzen 7 5800XT 8-Core Processor"),
            0.0
        );
        assert_eq!(brand_tctl_offset(""), 0.0);
    }

    #[test]
    fn ccd_registers_depend_on_model() {
        assert!(supports_per_ccd(0x21));
        assert!(!supports_per_ccd(0x08));
        assert_eq!(ccd_register(0x21, 0), CCD1_TEMP_M70H);
        assert_eq!(ccd_register(0x21, 1), CCD1_TEMP_M70H + 4);
        assert_eq!(ccd_register(0x61, 0), CCD1_TEMP_M61H);
    }

    #[test]
    fn ccd_temperature_filters_empty_and_absurd_slots() {
        assert_eq!(ccd_temp_from_raw(0), None);
        assert_eq!(ccd_temp_from_raw(0xFFF), None);
        assert_eq!(ccd_temp_from_raw(2840), Some(50.0));
        assert_eq!(ccd_temp_from_raw(0xF000 | 2840), Some(50.0));
    }

    #[test]
    fn the_energy_unit_comes_from_bits_12_to_8_of_the_rapl_unit_msr() {
        assert_eq!(energy_unit_joules(0x000A_1003), Some(1.0 / 65536.0));
        assert_eq!(energy_unit_joules(0x000A_0F03), Some(1.0 / 32768.0));
        assert_eq!(energy_unit_joules(0x0000_1F00), Some(1.0 / 2147483648.0));
    }

    #[test]
    fn an_energy_unit_exponent_of_zero_is_rejected_rather_than_read_as_one_joule() {
        assert_eq!(energy_unit_joules(0x0000_0000), None);
        assert_eq!(energy_unit_joules(0x000A_0003), None);
    }

    #[test]
    fn the_accumulator_modulus_is_one_past_the_widest_32_bit_value() {
        assert_eq!(ENERGY_COUNTER_MODULUS, 1u64 << 32);
    }

    #[test]
    fn the_package_accumulator_keeps_only_its_low_32_bits() {
        assert_eq!(package_energy_counter(0xDEAD_BEEF_1234_5678), 0x1234_5678);
        assert_eq!(package_energy_counter(7), 7);
    }
}
