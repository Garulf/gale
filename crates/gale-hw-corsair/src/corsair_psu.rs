use crate::transport::HidTransport;
use crate::{CorsairDevice, DeviceChannels};
use gale_hw::SensorKind;
use std::collections::HashMap;

const REPORT_LENGTH: usize = 64;
const READ_TIMEOUT_MS: i32 = 1000;

const SLAVE_ADDRESS: u8 = 0x02;
const WRITE_BIT: u8 = 0x00;
const READ_BIT: u8 = 0x01;

const CMD_PAGE: u8 = 0x00;
const CMD_FAN_COMMAND_1: u8 = 0x3b;
const CMD_READ_TEMPERATURE_1: u8 = 0x8d;
const CMD_READ_TEMPERATURE_2: u8 = 0x8e;
const CMD_READ_FAN_SPEED_1: u8 = 0x90;
const CMD_FAN_CONTROL_MODE: u8 = 0xf0;

const FAN_MODE_HARDWARE: u8 = 0x00;
const FAN_MODE_SOFTWARE: u8 = 0x01;

const MIN_FAN_DUTY_PCT: f64 = 30.0;

fn linear_to_float(bytes: &[u8]) -> f64 {
    let raw = u16::from_le_bytes([bytes[0], bytes[1]]) as i32;
    let mut exp = raw >> 11;
    let mut fra = raw & 0x7ff;
    if fra > 1023 {
        fra -= 2048;
    }
    if exp > 15 {
        exp -= 32;
    }
    fra as f64 * 2f64.powi(exp)
}

pub struct CorsairPsu {
    transport: Box<dyn HidTransport>,
    slug: String,
}

impl CorsairPsu {
    pub fn new(transport: Box<dyn HidTransport>, slug: impl Into<String>) -> Self {
        Self {
            transport,
            slug: slug.into(),
        }
    }

    fn exec(&mut self, writebit: u8, command: u8, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut frame = vec![0u8; REPORT_LENGTH];
        frame[0] = SLAVE_ADDRESS | writebit;
        frame[1] = command;
        let start = 2;
        let end = (start + data.len()).min(REPORT_LENGTH);
        frame[start..end].copy_from_slice(&data[..end - start]);
        self.transport.drain();
        self.transport.write(&frame)?;
        let response = self
            .transport
            .read_timeout(READ_TIMEOUT_MS)?
            .ok_or_else(|| "no response from device".to_string())?;
        if response.len() < 2 || response[0] != frame[0] || response[1] != frame[1] {
            return Err("invalid response (possible conflict with another program)".to_string());
        }
        Ok(response)
    }

    fn get_float(&mut self, command: u8) -> Option<f64> {
        let response = self.exec(READ_BIT, command, &[]).ok()?;
        if response.len() < 4 {
            return None;
        }
        Some(linear_to_float(&response[2..4]))
    }

    fn set_fan_control_mode(&mut self, mode: u8) -> Result<(), String> {
        self.exec(WRITE_BIT, CMD_FAN_CONTROL_MODE, &[mode])?;
        Ok(())
    }
}

impl CorsairDevice for CorsairPsu {
    fn slug(&self) -> &str {
        &self.slug
    }

    fn channels(&mut self) -> Result<DeviceChannels, String> {
        Ok(DeviceChannels {
            sensors: vec![
                (
                    "temp1".to_string(),
                    SensorKind::Temp,
                    "VRM temperature".to_string(),
                ),
                (
                    "temp2".to_string(),
                    SensorKind::Temp,
                    "Case temperature".to_string(),
                ),
                ("fan1".to_string(), SensorKind::Rpm, "Fan speed".to_string()),
            ],
            controls: vec![("fan1".to_string(), "Fan duty".to_string())],
        })
    }

    fn read(&mut self) -> HashMap<String, Option<f64>> {
        let mut values = HashMap::new();
        let _ = self.exec(WRITE_BIT, CMD_PAGE, &[0]);
        values.insert("temp1".to_string(), self.get_float(CMD_READ_TEMPERATURE_1));
        values.insert("temp2".to_string(), self.get_float(CMD_READ_TEMPERATURE_2));
        values.insert("fan1".to_string(), self.get_float(CMD_READ_FAN_SPEED_1));
        values
    }

    fn set_duty(&mut self, channel: &str, pct: f64) -> Result<(), String> {
        if channel != "fan1" {
            return Err(format!("unknown channel {channel}"));
        }
        let duty = pct.clamp(MIN_FAN_DUTY_PCT, 100.0).round() as u8;
        self.set_fan_control_mode(FAN_MODE_SOFTWARE)?;
        self.exec(WRITE_BIT, CMD_FAN_COMMAND_1, &[duty])?;
        Ok(())
    }

    fn release(&mut self, channel: &str) -> Result<(), String> {
        if channel != "fan1" {
            return Err(format!("unknown channel {channel}"));
        }
        self.set_fan_control_mode(FAN_MODE_HARDWARE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FakeTransport;

    fn frame(address_bit: u8, command: u8, data: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; REPORT_LENGTH];
        buf[0] = SLAVE_ADDRESS | address_bit;
        buf[1] = command;
        buf[2..2 + data.len()].copy_from_slice(data);
        buf
    }

    fn read_reply(command: u8, data: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; REPORT_LENGTH];
        buf[0] = SLAVE_ADDRESS | READ_BIT;
        buf[1] = command;
        buf[2..2 + data.len()].copy_from_slice(data);
        buf
    }

    fn write_ack(command: u8, data: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; REPORT_LENGTH];
        buf[0] = SLAVE_ADDRESS | WRITE_BIT;
        buf[1] = command;
        buf[2..2 + data.len()].copy_from_slice(data);
        buf
    }

    fn psu(exchanges: Vec<(Vec<u8>, Option<Vec<u8>>)>) -> CorsairPsu {
        CorsairPsu::new(Box::new(FakeTransport::new(exchanges)), "hx1000i")
    }

    // LINEAR11 vectors from liquidctl's pmbus.linear_to_float doctest
    #[test]
    fn linear_to_float_matches_liquidctl_doctest_vector_one() {
        assert_eq!(linear_to_float(&[0x67, 0xe3]), 54.4375);
    }

    #[test]
    fn linear_to_float_matches_liquidctl_doctest_negative_vector() {
        // float_to_linear11(-2812) round-trips through linear_to_float in
        // liquidctl's doctest; -2812 encodes to exponent 2, mantissa -703
        // (i.e. 1345 in 11-bit two's complement: 2048 - 703 = 1345),
        // little-endian bytes 41 15
        assert_eq!(linear_to_float(&[0x41, 0x15]), -2812.0);
    }

    // remaining vectors from liquidctl's test_corsair_hid_psu.py
    // SAMPLE_RESPONSES, cross-referenced against
    // test_reads_status_directly's expected temperatures and fan speed
    #[test]
    fn linear_to_float_decodes_vrm_temperature_sample() {
        // '038d86f0': command 0x8d (READ_TEMPERATURE_1), data 86 f0
        assert_eq!(linear_to_float(&[0x86, 0xf0]), 33.5);
    }

    #[test]
    fn linear_to_float_decodes_case_temperature_sample() {
        // '038e6af0': command 0x8e (READ_TEMPERATURE_2), data 6a f0
        assert_eq!(linear_to_float(&[0x6a, 0xf0]), 26.5);
    }

    #[test]
    fn linear_to_float_decodes_fan_speed_sample() {
        // '0390c803': command 0x90 (READ_FAN_SPEED_1), data c8 03
        assert_eq!(linear_to_float(&[0xc8, 0x03]), 968.0);
    }

    #[test]
    fn channels_lists_two_temps_and_one_fan() {
        let mut device = psu(vec![]);
        let channels = device.channels().unwrap();

        let sensor_ids: Vec<_> = channels
            .sensors
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect();
        assert_eq!(sensor_ids, vec!["temp1", "temp2", "fan1"]);

        let control_ids: Vec<_> = channels
            .controls
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(control_ids, vec!["fan1"]);
    }

    #[test]
    fn read_reports_vrm_temp_case_temp_and_fan_speed_in_native_units() {
        let exchanges = vec![
            (
                frame(WRITE_BIT, CMD_PAGE, &[0]),
                Some(write_ack(CMD_PAGE, &[0])),
            ),
            (
                frame(READ_BIT, CMD_READ_TEMPERATURE_1, &[]),
                Some(read_reply(CMD_READ_TEMPERATURE_1, &[0x86, 0xf0])),
            ),
            (
                frame(READ_BIT, CMD_READ_TEMPERATURE_2, &[]),
                Some(read_reply(CMD_READ_TEMPERATURE_2, &[0x6a, 0xf0])),
            ),
            (
                frame(READ_BIT, CMD_READ_FAN_SPEED_1, &[]),
                Some(read_reply(CMD_READ_FAN_SPEED_1, &[0xc8, 0x03])),
            ),
        ];
        let mut device = psu(exchanges);

        let values = device.read();

        assert_eq!(values["temp1"], Some(33.5));
        assert_eq!(values["temp2"], Some(26.5));
        assert_eq!(values["fan1"], Some(968.0));
    }

    #[test]
    fn read_reports_none_when_a_reply_times_out() {
        let exchanges = vec![
            (
                frame(WRITE_BIT, CMD_PAGE, &[0]),
                Some(write_ack(CMD_PAGE, &[0])),
            ),
            (frame(READ_BIT, CMD_READ_TEMPERATURE_1, &[]), None),
            (
                frame(READ_BIT, CMD_READ_TEMPERATURE_2, &[]),
                Some(read_reply(CMD_READ_TEMPERATURE_2, &[0x6a, 0xf0])),
            ),
            (
                frame(READ_BIT, CMD_READ_FAN_SPEED_1, &[]),
                Some(read_reply(CMD_READ_FAN_SPEED_1, &[0xc8, 0x03])),
            ),
        ];
        let mut device = psu(exchanges);

        let values = device.read();

        assert_eq!(values["temp1"], None);
        assert_eq!(values["temp2"], Some(26.5));
    }

    #[test]
    fn set_duty_switches_to_software_mode_then_writes_fixed_duty() {
        let exchanges = vec![
            (
                frame(WRITE_BIT, CMD_FAN_CONTROL_MODE, &[FAN_MODE_SOFTWARE]),
                Some(write_ack(CMD_FAN_CONTROL_MODE, &[FAN_MODE_SOFTWARE])),
            ),
            (
                frame(WRITE_BIT, CMD_FAN_COMMAND_1, &[60]),
                Some(write_ack(CMD_FAN_COMMAND_1, &[60])),
            ),
        ];
        let mut device = psu(exchanges);

        device.set_duty("fan1", 60.0).unwrap();
    }

    #[test]
    fn set_duty_enforces_minimum_thirty_percent_duty() {
        // liquidctl's test_enforce_minimum_user_set_fan_duty: duty=20 -> 30
        let exchanges = vec![
            (
                frame(WRITE_BIT, CMD_FAN_CONTROL_MODE, &[FAN_MODE_SOFTWARE]),
                Some(write_ack(CMD_FAN_CONTROL_MODE, &[FAN_MODE_SOFTWARE])),
            ),
            (
                frame(WRITE_BIT, CMD_FAN_COMMAND_1, &[30]),
                Some(write_ack(CMD_FAN_COMMAND_1, &[30])),
            ),
        ];
        let mut device = psu(exchanges);

        device.set_duty("fan1", 20.0).unwrap();
    }

    #[test]
    fn set_duty_clamps_above_hundred() {
        let exchanges = vec![
            (
                frame(WRITE_BIT, CMD_FAN_CONTROL_MODE, &[FAN_MODE_SOFTWARE]),
                Some(write_ack(CMD_FAN_CONTROL_MODE, &[FAN_MODE_SOFTWARE])),
            ),
            (
                frame(WRITE_BIT, CMD_FAN_COMMAND_1, &[100]),
                Some(write_ack(CMD_FAN_COMMAND_1, &[100])),
            ),
        ];
        let mut device = psu(exchanges);

        device.set_duty("fan1", 140.0).unwrap();
    }

    #[test]
    fn set_duty_rejects_unknown_channel() {
        let mut device = psu(vec![]);
        assert!(device.set_duty("temp1", 50.0).is_err());
        assert!(device.set_duty("fan2", 50.0).is_err());
    }

    #[test]
    fn release_restores_hardware_fan_control_mode() {
        let exchanges = vec![(
            frame(WRITE_BIT, CMD_FAN_CONTROL_MODE, &[FAN_MODE_HARDWARE]),
            Some(write_ack(CMD_FAN_CONTROL_MODE, &[FAN_MODE_HARDWARE])),
        )];
        let mut device = psu(exchanges);

        device.release("fan1").unwrap();
    }

    #[test]
    fn release_rejects_unknown_channel() {
        let mut device = psu(vec![]);
        assert!(device.release("temp1").is_err());
    }

    #[test]
    fn exec_drains_stale_reports_before_every_write() {
        let exchanges = vec![
            (
                frame(WRITE_BIT, CMD_FAN_CONTROL_MODE, &[FAN_MODE_SOFTWARE]),
                Some(write_ack(CMD_FAN_CONTROL_MODE, &[FAN_MODE_SOFTWARE])),
            ),
            (
                frame(WRITE_BIT, CMD_FAN_COMMAND_1, &[60]),
                Some(write_ack(CMD_FAN_COMMAND_1, &[60])),
            ),
        ];
        let transport = FakeTransport::new(exchanges);
        let writes = transport.write_counter();
        let drains = transport.drain_counter();
        let mut device = CorsairPsu::new(Box::new(transport), "hx1000i");

        device.set_duty("fan1", 60.0).unwrap();

        assert_eq!(
            drains.load(std::sync::atomic::Ordering::Relaxed),
            writes.load(std::sync::atomic::Ordering::Relaxed)
        );
    }

    #[test]
    fn exec_errors_when_reply_address_or_command_does_not_echo_the_request() {
        let exchanges = vec![(
            frame(READ_BIT, CMD_READ_TEMPERATURE_1, &[]),
            Some(read_reply(CMD_READ_TEMPERATURE_2, &[0x00, 0x00])),
        )];
        let mut device = psu(exchanges);

        let result = device.exec(READ_BIT, CMD_READ_TEMPERATURE_1, &[]);
        assert!(result.is_err());
    }
}
