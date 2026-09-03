use crate::transport::HidTransport;
use crate::{CorsairDevice, DeviceChannels};
use gale_hw::SensorKind;
use std::collections::HashMap;

const REPORT_LENGTH: usize = 64;
const READ_TIMEOUT_MS: i32 = 1000;

const WRITE_PREFIX: u8 = 0x3f;

const FEATURE_COOLING: u8 = 0b000;
const FEATURE_COOLING2: u8 = 0b011;
const CMD_GET_STATUS: u8 = 0xff;
const CMD_SET_COOLING: u8 = 0x14;

const SET_COOLING_DATA_LENGTH: usize = REPORT_LENGTH - 4;
const SET_COOLING_DATA_PREFIX: [u8; 8] = [0x00, 0xff, 0x05, 0xff, 0xff, 0xff, 0xff, 0xff];
const FAN_MODE_OFFSETS: [usize; 2] = [0x0b - 3, 0x11 - 3];
const FAN_DUTY_OFFSETS: [usize; 2] = [FAN_MODE_OFFSETS[0] + 5, FAN_MODE_OFFSETS[1] + 5];
const PUMP_MODE_OFFSET: usize = 0x17 - 3;
const PROFILE_LENGTH_OFFSET: usize = 0x1d - 3;
const PROFILE_LENGTH: u8 = 7;

const FAN_MODE_FIXED_DUTY: u8 = 0x2;
const PUMP_MODE_BALANCED: u8 = 0x1;
const PUMP_MODE_UNUSED: u8 = 0xff;
const RELEASE_DUTY_PCT: u8 = 100;
const DEFAULT_DUTY_PCT: u8 = 100;

const TEMP_FRAC_OFFSET: usize = 7;
const TEMP_INT_OFFSET: usize = 8;
const FAN_SPEED_OFFSETS: [usize; 3] = [15, 22, 43];
const PUMP_SPEED_OFFSET: usize = 29;

const SEQUENCE_MAX: u8 = 31;

fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn fraction_of_byte(percentage: u8) -> u8 {
    ((percentage as f64 / 100.0) * 255.0).round_ties_even() as u8
}

fn u16le_from(data: &[u8], offset: usize) -> Option<u16> {
    let lo = *data.get(offset)?;
    let hi = *data.get(offset + 1)?;
    Some(u16::from_le_bytes([lo, hi]))
}

pub struct HydroPlatinum {
    transport: Box<dyn HidTransport>,
    slug: String,
    fan_count: usize,
    sequence: u8,
    fan_duty: Vec<Option<u8>>,
}

impl HydroPlatinum {
    pub fn new(
        transport: Box<dyn HidTransport>,
        slug: impl Into<String>,
        fan_count: usize,
    ) -> Self {
        Self {
            transport,
            slug: slug.into(),
            fan_count,
            sequence: 0,
            fan_duty: vec![None; fan_count],
        }
    }

    fn next_sequence(&mut self) -> u8 {
        self.sequence = self.sequence % SEQUENCE_MAX + 1;
        self.sequence
    }

    fn send_command(&mut self, feature: u8, command: u8, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut frame = vec![0u8; REPORT_LENGTH];
        frame[0] = WRITE_PREFIX;
        let seq = self.next_sequence();
        frame[1] = (seq << 3) | feature;
        frame[2] = command;
        let start = 3;
        let end = (start + data.len()).min(REPORT_LENGTH - 1);
        frame[start..end].copy_from_slice(&data[..end - start]);
        let last = REPORT_LENGTH - 1;
        frame[last] = crc8(&frame[1..last]);
        self.transport.drain();
        self.transport.write(&frame)?;
        self.transport
            .read_timeout(READ_TIMEOUT_MS)?
            .ok_or_else(|| "no response from device".to_string())
    }

    fn cooling_data(&self, duties: &[Option<u8>], pump_byte: u8) -> Vec<u8> {
        let mut data = vec![0u8; SET_COOLING_DATA_LENGTH];
        data[0..SET_COOLING_DATA_PREFIX.len()].copy_from_slice(&SET_COOLING_DATA_PREFIX);
        data[PROFILE_LENGTH_OFFSET] = PROFILE_LENGTH;
        data[PUMP_MODE_OFFSET] = pump_byte;
        for (i, duty) in duties.iter().enumerate() {
            let pct = duty.unwrap_or(DEFAULT_DUTY_PCT);
            data[FAN_MODE_OFFSETS[i]] = FAN_MODE_FIXED_DUTY;
            data[FAN_DUTY_OFFSETS[i]] = fraction_of_byte(pct);
        }
        data
    }

    fn send_cooling(&mut self) -> Result<(), String> {
        if self.fan_count == 3 {
            let data = self.cooling_data(&self.fan_duty[2..3], PUMP_MODE_UNUSED);
            self.send_command(FEATURE_COOLING2, CMD_SET_COOLING, &data)?;
        }
        let data = self.cooling_data(&self.fan_duty[0..2], PUMP_MODE_BALANCED);
        self.send_command(FEATURE_COOLING, CMD_SET_COOLING, &data)?;
        Ok(())
    }

    fn read_status(&mut self) -> Option<Vec<u8>> {
        self.send_command(FEATURE_COOLING, CMD_GET_STATUS, &[]).ok()
    }

    fn parse_fan_index(&self, channel: &str) -> Result<usize, String> {
        let n: usize = channel
            .strip_prefix("fan")
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("unknown channel {channel}"))?;
        if !(1..=self.fan_count).contains(&n) {
            return Err(format!("unknown channel {channel}"));
        }
        Ok(n - 1)
    }
}

impl CorsairDevice for HydroPlatinum {
    fn slug(&self) -> &str {
        &self.slug
    }

    fn channels(&mut self) -> Result<DeviceChannels, String> {
        let mut sensors = Vec::new();
        let mut controls = Vec::new();
        for i in 0..self.fan_count {
            let id = format!("fan{}", i + 1);
            sensors.push((id.clone(), SensorKind::Rpm, format!("Fan {} speed", i + 1)));
            controls.push((id, format!("Fan {} duty", i + 1)));
        }
        sensors.push((
            "pump".to_string(),
            SensorKind::Rpm,
            "Pump speed".to_string(),
        ));
        sensors.push((
            "temp1".to_string(),
            SensorKind::Temp,
            "Coolant temperature".to_string(),
        ));
        Ok(DeviceChannels { sensors, controls })
    }

    fn read(&mut self) -> HashMap<String, Option<f64>> {
        let res = self.read_status();
        let mut values = HashMap::new();

        let temp = res.as_ref().and_then(|res| {
            let int_part = *res.get(TEMP_INT_OFFSET)? as f64;
            let frac = *res.get(TEMP_FRAC_OFFSET)? as f64;
            Some(int_part + frac / 255.0)
        });
        values.insert("temp1".to_string(), temp);

        for (i, offset) in FAN_SPEED_OFFSETS.iter().enumerate().take(self.fan_count) {
            let speed = res
                .as_ref()
                .and_then(|res| u16le_from(res, *offset))
                .map(f64::from);
            values.insert(format!("fan{}", i + 1), speed);
        }

        let pump_speed = res
            .as_ref()
            .and_then(|res| u16le_from(res, PUMP_SPEED_OFFSET))
            .map(f64::from);
        values.insert("pump".to_string(), pump_speed);

        values
    }

    fn set_duty(&mut self, channel: &str, pct: f64) -> Result<(), String> {
        let index = self.parse_fan_index(channel)?;
        let duty = pct.clamp(0.0, 100.0).round() as u8;
        self.fan_duty[index] = Some(duty);
        self.send_cooling()
    }

    fn release(&mut self, channel: &str) -> Result<(), String> {
        let index = self.parse_fan_index(channel)?;
        self.fan_duty[index] = Some(RELEASE_DUTY_PCT);
        self.send_cooling()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FakeTransport;

    type Exchange = (Vec<u8>, Option<Vec<u8>>);

    fn expected_frame(seq: u8, feature: u8, command: u8, data: &[u8]) -> Vec<u8> {
        let mut frame = vec![0u8; REPORT_LENGTH];
        frame[0] = WRITE_PREFIX;
        frame[1] = (seq << 3) | feature;
        frame[2] = command;
        frame[3..3 + data.len()].copy_from_slice(data);
        let last = REPORT_LENGTH - 1;
        frame[last] = crc8(&frame[1..last]);
        frame
    }

    #[test]
    fn fraction_of_byte_uses_banker_rounding_like_liquidctl() {
        assert_eq!(fraction_of_byte(30), 76);
        assert_eq!(fraction_of_byte(70), 178);
    }

    fn ack(command: u8) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; REPORT_LENGTH];
        buf[2] = command;
        Some(buf)
    }

    fn status_frame(temp: f64, fan_speeds: &[u16], pump_speed: u16) -> Vec<u8> {
        let mut buf = vec![0u8; REPORT_LENGTH];
        let int_part = temp.trunc() as u8;
        let frac = ((temp - temp.trunc()) * 255.0).round() as u8;
        buf[TEMP_FRAC_OFFSET] = frac;
        buf[TEMP_INT_OFFSET] = int_part;
        for (i, speed) in fan_speeds.iter().enumerate() {
            let offset = FAN_SPEED_OFFSETS[i];
            buf[offset..offset + 2].copy_from_slice(&speed.to_le_bytes());
        }
        buf[PUMP_SPEED_OFFSET..PUMP_SPEED_OFFSET + 2].copy_from_slice(&pump_speed.to_le_bytes());
        buf
    }

    fn hydro(fan_count: usize, exchanges: Vec<Exchange>) -> HydroPlatinum {
        HydroPlatinum::new(
            Box::new(FakeTransport::new(exchanges)),
            "h115i-platinum",
            fan_count,
        )
    }

    #[test]
    fn crc8_matches_known_good_liquidctl_status_frame() {
        // fixture from liquidctl's test_hydro_platinum.py,
        // test_h115i_platinum_device_handle_real_statuses
        let sample = "ff08110f0001002c1e0000aee803aed10700aee803aece0701aa0000aa9c0900\
0000000000000000000000000000000000000000000000000000000000000010";
        let bytes: Vec<u8> = (0..sample.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&sample[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(bytes.len(), 64);
        assert_eq!(crc8(&bytes[1..]), 0);
    }

    #[test]
    fn crc8_second_known_good_liquidctl_status_frame() {
        let sample = "ff40110f009e14011b0102ffe8037e6a0502ffe8037e6d0501aa0000aa350901\
0000000000000000000000000000000000000000000000000000000000000098";
        let bytes: Vec<u8> = (0..sample.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&sample[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(bytes.len(), 64);
        assert_eq!(crc8(&bytes[1..]), 0);
    }

    #[test]
    fn sequence_numbers_wrap_after_thirty_one() {
        let mut dev = hydro(2, vec![]);
        for expected in 1..=31u8 {
            assert_eq!(dev.next_sequence(), expected);
        }
        for expected in 1..=31u8 {
            assert_eq!(dev.next_sequence(), expected);
        }
    }

    #[test]
    fn channels_list_two_fans_pump_and_coolant_temp() {
        let mut dev = hydro(2, vec![]);
        let channels = dev.channels().unwrap();

        let sensor_ids: Vec<_> = channels
            .sensors
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect();
        assert_eq!(sensor_ids, vec!["fan1", "fan2", "pump", "temp1"]);

        let control_ids: Vec<_> = channels
            .controls
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(control_ids, vec!["fan1", "fan2"]);
    }

    #[test]
    fn channels_list_three_fans_for_h150i_family() {
        let mut dev = hydro(3, vec![]);
        let channels = dev.channels().unwrap();

        let sensor_ids: Vec<_> = channels
            .sensors
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect();
        assert_eq!(sensor_ids, vec!["fan1", "fan2", "fan3", "pump", "temp1"]);

        let control_ids: Vec<_> = channels
            .controls
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(control_ids, vec!["fan1", "fan2", "fan3"]);
    }

    #[test]
    fn read_reports_temp_celsius_and_rpm_from_status_frame() {
        let status = status_frame(30.9, &[1499, 1512], 2702);
        let expected_write = expected_frame(1, FEATURE_COOLING, CMD_GET_STATUS, &[]);
        let mut dev = hydro(2, vec![(expected_write, Some(status))]);

        let values = dev.read();

        assert_eq!(values["fan1"], Some(1499.0));
        assert_eq!(values["fan2"], Some(1512.0));
        assert_eq!(values["pump"], Some(2702.0));
        assert!((values["temp1"].unwrap() - 30.9).abs() < 0.01);
    }

    #[test]
    fn read_reports_none_when_status_read_times_out() {
        let expected_write = expected_frame(1, FEATURE_COOLING, CMD_GET_STATUS, &[]);
        let mut dev = hydro(2, vec![(expected_write, None)]);

        let values = dev.read();

        assert_eq!(values["temp1"], None);
        assert_eq!(values["fan1"], None);
        assert_eq!(values["fan2"], None);
        assert_eq!(values["pump"], None);
    }

    #[test]
    fn set_duty_writes_fixed_duty_cooling_frame_for_two_fan_device() {
        let duties = [None, Some(60)];
        let data = HydroPlatinum::new(Box::new(FakeTransport::new(vec![])), "x", 2)
            .cooling_data(&duties, PUMP_MODE_BALANCED);
        let expected_write = expected_frame(1, FEATURE_COOLING, CMD_SET_COOLING, &data);

        let mut dev = hydro(2, vec![(expected_write, ack(CMD_SET_COOLING))]);
        dev.set_duty("fan2", 60.0).unwrap();

        assert_eq!(dev.fan_duty, vec![None, Some(60)]);
    }

    #[test]
    fn set_duty_preserves_other_fan_duty_across_calls() {
        let first_data = HydroPlatinum::new(Box::new(FakeTransport::new(vec![])), "x", 2)
            .cooling_data(&[Some(42), None], PUMP_MODE_BALANCED);
        let first_write = expected_frame(1, FEATURE_COOLING, CMD_SET_COOLING, &first_data);

        let second_data = HydroPlatinum::new(Box::new(FakeTransport::new(vec![])), "x", 2)
            .cooling_data(&[Some(42), Some(84)], PUMP_MODE_BALANCED);
        let second_write = expected_frame(2, FEATURE_COOLING, CMD_SET_COOLING, &second_data);

        let mut dev = hydro(
            2,
            vec![
                (first_write, ack(CMD_SET_COOLING)),
                (second_write, ack(CMD_SET_COOLING)),
            ],
        );
        dev.set_duty("fan1", 42.0).unwrap();
        dev.set_duty("fan2", 84.0).unwrap();
    }

    #[test]
    fn set_duty_writes_third_fan_frame_before_the_main_frame_on_h150i_family() {
        let fan3_data = HydroPlatinum::new(Box::new(FakeTransport::new(vec![])), "x", 3)
            .cooling_data(&[Some(50)], PUMP_MODE_UNUSED);
        let fan3_write = expected_frame(1, FEATURE_COOLING2, CMD_SET_COOLING, &fan3_data);

        let main_data = HydroPlatinum::new(Box::new(FakeTransport::new(vec![])), "x", 3)
            .cooling_data(&[None, None], PUMP_MODE_BALANCED);
        let main_write = expected_frame(2, FEATURE_COOLING, CMD_SET_COOLING, &main_data);

        let mut dev = hydro(
            3,
            vec![
                (fan3_write, ack(CMD_SET_COOLING)),
                (main_write, ack(CMD_SET_COOLING)),
            ],
        );
        dev.set_duty("fan3", 50.0).unwrap();
    }

    #[test]
    fn set_duty_clamps_percentages_outside_zero_to_hundred() {
        let low_data = HydroPlatinum::new(Box::new(FakeTransport::new(vec![])), "x", 2)
            .cooling_data(&[Some(0), None], PUMP_MODE_BALANCED);
        let low_write = expected_frame(1, FEATURE_COOLING, CMD_SET_COOLING, &low_data);

        let high_data = HydroPlatinum::new(Box::new(FakeTransport::new(vec![])), "x", 2)
            .cooling_data(&[Some(100), None], PUMP_MODE_BALANCED);
        let high_write = expected_frame(2, FEATURE_COOLING, CMD_SET_COOLING, &high_data);

        let mut dev = hydro(
            2,
            vec![
                (low_write, ack(CMD_SET_COOLING)),
                (high_write, ack(CMD_SET_COOLING)),
            ],
        );
        dev.set_duty("fan1", -10.0).unwrap();
        dev.set_duty("fan1", 140.0).unwrap();
    }

    #[test]
    fn set_duty_rejects_pump_and_unknown_channels() {
        let mut dev = hydro(2, vec![]);
        assert!(dev.set_duty("pump", 50.0).is_err());
        assert!(dev.set_duty("fan3", 50.0).is_err());
        assert!(dev.set_duty("fan", 50.0).is_err());
        assert!(dev.set_duty("temp1", 50.0).is_err());
    }

    #[test]
    fn release_sets_full_duty_as_documented_fallback() {
        let data = HydroPlatinum::new(Box::new(FakeTransport::new(vec![])), "x", 2)
            .cooling_data(&[None, Some(100)], PUMP_MODE_BALANCED);
        let expected_write = expected_frame(1, FEATURE_COOLING, CMD_SET_COOLING, &data);

        let mut dev = hydro(2, vec![(expected_write, ack(CMD_SET_COOLING))]);
        dev.release("fan2").unwrap();

        assert_eq!(dev.fan_duty, vec![None, Some(100)]);
    }

    #[test]
    fn send_command_drains_stale_reports_before_every_write() {
        let expected_write = expected_frame(1, FEATURE_COOLING, CMD_GET_STATUS, &[]);
        let transport = FakeTransport::new(vec![(expected_write, ack(CMD_GET_STATUS))]);
        let writes = transport.write_counter();
        let drains = transport.drain_counter();
        let mut dev = HydroPlatinum::new(Box::new(transport), "h115i-platinum", 2);

        dev.read();

        assert_eq!(
            drains.load(std::sync::atomic::Ordering::Relaxed),
            writes.load(std::sync::atomic::Ordering::Relaxed)
        );
    }

    #[test]
    fn get_status_write_has_no_data_and_cooling_feature() {
        let expected_write = expected_frame(1, FEATURE_COOLING, CMD_GET_STATUS, &[]);
        let mut dev = hydro(2, vec![(expected_write, ack(CMD_GET_STATUS))]);
        dev.read();
    }
}
