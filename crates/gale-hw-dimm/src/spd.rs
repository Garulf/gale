// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from RAMSPDToolkit (Blacktempel/RAMSPDToolkit), SPD/DDR4Accessor.cs,
// SPD/DDR5Accessor.cs and SPD/Interop.

pub const SPD_FIRST: u8 = 0x50;
pub const SPD_LAST: u8 = 0x57;
pub const DDR4_PAGE_ADDRESS: u8 = 0x36;
pub const DDR4_MEMORY_TYPE_BYTE: u8 = 0x02;
pub const DDR4_THERMAL_SENSOR_BYTE: u8 = 0x0E;
pub const DDR4_THERMAL_SENSOR_BIT: u8 = 0x80;
pub const DDR4_TS_CAPABILITIES_REGISTER: u8 = 0x00;
pub const DDR4_TS_TEMPERATURE_REGISTER: u8 = 0x05;
pub const DDR5_PAGE_REGISTER: u8 = 0x0B;
pub const DDR5_DEVICE_TYPE_MSB: u8 = 0x00;
pub const DDR5_DEVICE_TYPE_LSB: u8 = 0x01;
pub const DDR5_DEVICE_TYPE_MSB_EXPECTED: u8 = 0x51;
pub const DDR5_DEVICE_TYPE_LSB_EXPECTED: u8 = 0x18;
pub const DDR5_THERMAL_SENSOR_DISABLED: u8 = 0x1A;
pub const DDR5_TEMPERATURE_REGISTER: u8 = 0x31;

pub const DDR4_TYPES: [u8; 4] = [12, 14, 16, 17];

pub fn is_ddr4_type(memory_type: u8) -> bool {
    DDR4_TYPES.contains(&memory_type)
}

pub fn ddr4_thermal_sensor_address(spd_address: u8) -> u8 {
    0x18 | (spd_address & 0x07)
}

pub fn temperature_from_raw(raw: u16) -> f64 {
    let raw = raw & 0x1FFF;
    if raw & 0x1000 != 0 {
        f64::from(raw & 0xFFF) * 0.0625 - 256.0
    } else {
        f64::from(raw) * 0.0625
    }
}

pub fn swap_bytes(word: u16) -> u16 {
    word.rotate_left(8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thermal_sensor_sits_at_0x18_plus_slot() {
        assert_eq!(ddr4_thermal_sensor_address(0x50), 0x18);
        assert_eq!(ddr4_thermal_sensor_address(0x53), 0x1B);
    }

    #[test]
    fn temperature_decode_handles_sign_and_resolution() {
        assert_eq!(temperature_from_raw(0x0280), 40.0);
        assert_eq!(temperature_from_raw(0x0281), 40.0625);
        assert_eq!(temperature_from_raw(0x1FF0), -1.0);
        assert_eq!(temperature_from_raw(0xE280), 40.0);
    }

    #[test]
    fn swap_bytes_makes_the_high_byte_first() {
        assert_eq!(swap_bytes(0x8002), 0x0280);
    }
}
