use crate::transport::HidTransport;
use crate::{CorsairDevice, DeviceChannels, ReleaseMode};
use gale_hw::SensorKind;
use std::collections::HashMap;
use std::time::Duration;

const CMD_GET_FIRMWARE: u8 = 0x02;
const CMD_GET_BOOTLOADER: u8 = 0x06;
const CMD_GET_TEMP_CONFIG: u8 = 0x10;
const CMD_GET_TEMP: u8 = 0x11;
const CMD_GET_FAN_MODES: u8 = 0x20;
const CMD_GET_FAN_RPM: u8 = 0x21;
const CMD_SET_FAN_DUTY: u8 = 0x23;

const FAN_MODE_DC: u8 = 0x01;
const FAN_MODE_PWM: u8 = 0x02;

const REPORT_LENGTH: usize = 64;
const READ_TIMEOUT_MS: i32 = 1000;

const TEMP_PROBE_COUNT: usize = 4;
const FAN_COUNT: usize = 6;

const RELEASE_DUTY_PCT: u8 = 100;

const MAX_DISCOVERY_ATTEMPTS: usize = 3;
const DISCOVERY_RETRY_DELAY: Duration = Duration::from_millis(50);

pub struct CommanderPro {
    transport: Box<dyn HidTransport>,
    slug: String,
    release_mode: ReleaseMode,
    temp_connected: [bool; TEMP_PROBE_COUNT],
    fan_present: [bool; FAN_COUNT],
}

impl CommanderPro {
    pub fn new(
        transport: Box<dyn HidTransport>,
        slug: impl Into<String>,
        release_mode: ReleaseMode,
    ) -> Self {
        Self {
            transport,
            slug: slug.into(),
            release_mode,
            temp_connected: [false; TEMP_PROBE_COUNT],
            fan_present: [false; FAN_COUNT],
        }
    }

    fn exchange(&mut self, command: u8, data: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let mut frame = vec![0u8; REPORT_LENGTH];
        frame[0] = command;
        let payload_len = data.len().min(REPORT_LENGTH - 1);
        frame[1..1 + payload_len].copy_from_slice(&data[..payload_len]);
        self.transport.drain();
        self.transport.write(&frame)?;
        self.transport.read_timeout(READ_TIMEOUT_MS)
    }

    fn read_agreeing_discovery<T, F>(
        &mut self,
        command: u8,
        decode: F,
        label: &str,
    ) -> Result<T, String>
    where
        T: PartialEq,
        F: Fn(Option<Vec<u8>>) -> T,
    {
        let mut last = None;
        for attempt in 0..MAX_DISCOVERY_ATTEMPTS {
            let first = decode(self.exchange(command, &[])?);
            let second = decode(self.exchange(command, &[])?);
            if first == second {
                return Ok(second);
            }
            last = Some(second);
            if attempt + 1 < MAX_DISCOVERY_ATTEMPTS {
                self.transport.sleep(DISCOVERY_RETRY_DELAY);
            }
        }
        tracing::warn!(
            "{label} discovery reads did not agree after {MAX_DISCOVERY_ATTEMPTS} attempts on {}; another program may be using this device",
            self.slug
        );
        last.ok_or_else(|| "discovery produced no reading".to_string())
    }

    fn probe(&mut self) -> Result<(), String> {
        self.exchange(CMD_GET_FIRMWARE, &[])?;
        self.exchange(CMD_GET_BOOTLOADER, &[])?;

        self.temp_connected = self.read_agreeing_discovery(
            CMD_GET_TEMP_CONFIG,
            decode_temp_connected,
            "temperature probe",
        )?;

        self.fan_present =
            self.read_agreeing_discovery(CMD_GET_FAN_MODES, decode_fan_present, "fan mode")?;

        Ok(())
    }

    fn read_temp(&mut self, index: usize) -> Option<f64> {
        match self.exchange(CMD_GET_TEMP, &[index as u8]) {
            Ok(Some(response)) if response.len() >= 3 => {
                Some(u16::from_be_bytes([response[1], response[2]]) as f64 / 100.0)
            }
            _ => None,
        }
    }

    fn read_fan_rpm(&mut self, index: usize) -> Option<f64> {
        match self.exchange(CMD_GET_FAN_RPM, &[index as u8]) {
            Ok(Some(response)) if response.len() >= 3 => {
                Some(u16::from_be_bytes([response[1], response[2]]) as f64)
            }
            _ => None,
        }
    }

    fn write_duty(&mut self, index: usize, pct: u8) -> Result<(), String> {
        self.exchange(CMD_SET_FAN_DUTY, &[index as u8, pct])?;
        Ok(())
    }
}

fn decode_temp_connected(response: Option<Vec<u8>>) -> [bool; TEMP_PROBE_COUNT] {
    let mut connected = [false; TEMP_PROBE_COUNT];
    if let Some(response) = response {
        for (i, c) in connected.iter_mut().enumerate() {
            *c = response.get(1 + i).copied().unwrap_or(0) != 0;
        }
    }
    connected
}

fn decode_fan_present(response: Option<Vec<u8>>) -> [bool; FAN_COUNT] {
    let mut present = [false; FAN_COUNT];
    if let Some(response) = response {
        for (i, p) in present.iter_mut().enumerate() {
            let mode = response.get(1 + i).copied().unwrap_or(0);
            *p = mode == FAN_MODE_DC || mode == FAN_MODE_PWM;
        }
    }
    present
}

fn parse_fan_index(channel: &str) -> Result<usize, String> {
    let n: usize = channel
        .strip_prefix("fan")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("unknown channel {channel}"))?;
    if !(1..=FAN_COUNT).contains(&n) {
        return Err(format!("unknown channel {channel}"));
    }
    Ok(n - 1)
}

impl CorsairDevice for CommanderPro {
    fn slug(&self) -> &str {
        &self.slug
    }

    fn channels(&mut self) -> Result<DeviceChannels, String> {
        self.probe()?;

        let mut sensors = Vec::new();
        for (i, connected) in self.temp_connected.iter().enumerate() {
            if *connected {
                sensors.push((
                    format!("temp{}", i + 1),
                    SensorKind::Temp,
                    format!("Temperature {}", i + 1),
                ));
            }
        }
        for (i, present) in self.fan_present.iter().enumerate() {
            if *present {
                sensors.push((
                    format!("fan{}", i + 1),
                    SensorKind::Rpm,
                    format!("Fan {}", i + 1),
                ));
            }
        }

        let mut controls = Vec::new();
        for (i, present) in self.fan_present.iter().enumerate() {
            if *present {
                controls.push((format!("fan{}", i + 1), format!("Fan {} PWM", i + 1)));
            }
        }

        Ok(DeviceChannels { sensors, controls })
    }

    fn read(&mut self) -> HashMap<String, Option<f64>> {
        let mut values = HashMap::new();

        for i in 0..TEMP_PROBE_COUNT {
            if self.temp_connected[i] {
                let value = self.read_temp(i);
                values.insert(format!("temp{}", i + 1), value);
            }
        }

        for i in 0..FAN_COUNT {
            if self.fan_present[i] {
                let value = self.read_fan_rpm(i);
                values.insert(format!("fan{}", i + 1), value);
            }
        }

        values
    }

    fn set_duty(&mut self, channel: &str, pct: f64) -> Result<(), String> {
        let index = parse_fan_index(channel)?;
        if !self.fan_present[index] {
            return Ok(());
        }
        let duty = pct.clamp(0.0, 100.0).round() as u8;
        self.write_duty(index, duty)
    }

    fn release(&mut self, channel: &str) -> Result<(), String> {
        let index = parse_fan_index(channel)?;
        if !self.fan_present[index] {
            return Ok(());
        }
        match self.release_mode {
            ReleaseMode::PinFull => self.write_duty(index, RELEASE_DUTY_PCT),
            ReleaseMode::KeepLast => Ok(()),
            ReleaseMode::Fixed(percent) => self.write_duty(index, percent),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FakeTransport;

    fn frame(command: u8, data: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; REPORT_LENGTH];
        buf[0] = command;
        buf[1..1 + data.len()].copy_from_slice(data);
        buf
    }

    fn padded_response(payload: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; 16];
        buf[..payload.len()].copy_from_slice(payload);
        buf
    }

    fn probe_exchanges(temp_config: &[u8], fan_modes: &[u8]) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        vec![
            (
                frame(CMD_GET_FIRMWARE, &[]),
                Some(padded_response(&[0x00, 0x00, 0x09, 0xd4])),
            ),
            (
                frame(CMD_GET_BOOTLOADER, &[]),
                Some(padded_response(&[0x00, 0x00, 0x05, 0x00])),
            ),
            (
                frame(CMD_GET_TEMP_CONFIG, &[]),
                Some(padded_response(temp_config)),
            ),
            (
                frame(CMD_GET_TEMP_CONFIG, &[]),
                Some(padded_response(temp_config)),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(fan_modes)),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(fan_modes)),
            ),
        ]
    }

    #[test]
    fn channels_accepts_discovery_reads_that_only_differ_in_unused_padding_bytes() {
        let mut first_reply = padded_response(&[0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00]);
        first_reply[15] = 0xaa;
        let mut second_reply = padded_response(&[0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00]);
        second_reply[15] = 0x55;
        assert_ne!(first_reply, second_reply);

        let exchanges = vec![
            (
                frame(CMD_GET_FIRMWARE, &[]),
                Some(padded_response(&[0x00, 0x00, 0x09, 0xd4])),
            ),
            (
                frame(CMD_GET_BOOTLOADER, &[]),
                Some(padded_response(&[0x00, 0x00, 0x05, 0x00])),
            ),
            (
                frame(CMD_GET_TEMP_CONFIG, &[]),
                Some(padded_response(&[0, 0, 0, 0, 0])),
            ),
            (
                frame(CMD_GET_TEMP_CONFIG, &[]),
                Some(padded_response(&[0, 0, 0, 0, 0])),
            ),
            (frame(CMD_GET_FAN_MODES, &[]), Some(first_reply)),
            (frame(CMD_GET_FAN_MODES, &[]), Some(second_reply)),
        ];
        let expected_exchanges = exchanges.len();
        let transport = FakeTransport::new(exchanges);
        let writes = transport.write_counter();
        let sleeps = transport.sleep_counter();
        let mut device =
            CommanderPro::new(Box::new(transport), "commander-pro", ReleaseMode::PinFull);

        let channels = device.channels().unwrap();

        let control_ids: Vec<_> = channels
            .controls
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(control_ids, vec!["fan1", "fan2"]);
        assert_eq!(
            writes.load(std::sync::atomic::Ordering::Relaxed),
            expected_exchanges
        );
        assert_eq!(sleeps.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn channels_retries_fan_mode_discovery_until_two_reads_agree() {
        let matching = [0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00];
        let exchanges = vec![
            (
                frame(CMD_GET_FIRMWARE, &[]),
                Some(padded_response(&[0x00, 0x00, 0x09, 0xd4])),
            ),
            (
                frame(CMD_GET_BOOTLOADER, &[]),
                Some(padded_response(&[0x00, 0x00, 0x05, 0x00])),
            ),
            (
                frame(CMD_GET_TEMP_CONFIG, &[]),
                Some(padded_response(&[0, 0, 0, 0, 0])),
            ),
            (
                frame(CMD_GET_TEMP_CONFIG, &[]),
                Some(padded_response(&[0, 0, 0, 0, 0])),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00])),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(&[0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00])),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(&matching)),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(&matching)),
            ),
        ];
        let expected_exchanges = exchanges.len();
        let transport = FakeTransport::new(exchanges);
        let writes = transport.write_counter();
        let sleeps = transport.sleep_counter();
        let mut device =
            CommanderPro::new(Box::new(transport), "commander-pro", ReleaseMode::PinFull);

        let channels = device.channels().unwrap();

        let control_ids: Vec<_> = channels
            .controls
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(control_ids, vec!["fan1", "fan2"]);
        assert_eq!(
            writes.load(std::sync::atomic::Ordering::Relaxed),
            expected_exchanges
        );
        assert_eq!(sleeps.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn channels_keeps_last_reading_and_gives_up_when_fan_mode_discovery_never_agrees() {
        let exchanges = vec![
            (
                frame(CMD_GET_FIRMWARE, &[]),
                Some(padded_response(&[0x00, 0x00, 0x09, 0xd4])),
            ),
            (
                frame(CMD_GET_BOOTLOADER, &[]),
                Some(padded_response(&[0x00, 0x00, 0x05, 0x00])),
            ),
            (
                frame(CMD_GET_TEMP_CONFIG, &[]),
                Some(padded_response(&[0, 0, 0, 0, 0])),
            ),
            (
                frame(CMD_GET_TEMP_CONFIG, &[]),
                Some(padded_response(&[0, 0, 0, 0, 0])),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00])),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(&[0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00])),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(&[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00])),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(&[0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00])),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00])),
            ),
            (
                frame(CMD_GET_FAN_MODES, &[]),
                Some(padded_response(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01])),
            ),
        ];
        let expected_exchanges = exchanges.len();
        let transport = FakeTransport::new(exchanges);
        let writes = transport.write_counter();
        let sleeps = transport.sleep_counter();
        let mut device =
            CommanderPro::new(Box::new(transport), "commander-pro", ReleaseMode::PinFull);

        let channels = device.channels().unwrap();

        let control_ids: Vec<_> = channels
            .controls
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(control_ids, vec!["fan6"]);
        assert_eq!(
            writes.load(std::sync::atomic::Ordering::Relaxed),
            expected_exchanges
        );
        assert_eq!(sleeps.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[test]
    fn channels_probes_temp_and_fan_presence() {
        let exchanges = probe_exchanges(
            &[0x00, 0x01, 0x01, 0x00, 0x01],
            &[0x00, 0x01, 0x01, 0x02, 0x00, 0x00, 0x00],
        );
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );

        let channels = device.channels().unwrap();

        let sensor_ids: Vec<_> = channels
            .sensors
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect();
        assert_eq!(
            sensor_ids,
            vec!["temp1", "temp2", "temp4", "fan1", "fan2", "fan3"]
        );

        let control_ids: Vec<_> = channels
            .controls
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(control_ids, vec!["fan1", "fan2", "fan3"]);
    }

    #[test]
    fn read_reports_temp_in_celsius_from_centidegrees() {
        let mut exchanges = probe_exchanges(&[0x00, 0x00, 0x01, 0x00, 0x00], &[0, 0, 0, 0, 0, 0]);
        exchanges.push((
            frame(CMD_GET_TEMP, &[1]),
            Some(padded_response(&[0x00, 0x0a, 0x83])),
        ));
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );
        device.channels().unwrap();

        let values = device.read();

        assert_eq!(values["temp2"], Some(26.91));
    }

    #[test]
    fn read_reports_fan_rpm_unconverted() {
        let mut exchanges = probe_exchanges(&[0, 0, 0, 0], &[0x00, 0x00, 0x01, 0x00, 0x00, 0x00]);
        exchanges.push((
            frame(CMD_GET_FAN_RPM, &[1]),
            Some(padded_response(&[0x00, 0x03, 0xac])),
        ));
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );
        device.channels().unwrap();

        let values = device.read();

        assert_eq!(values["fan2"], Some(940.0));
    }

    #[test]
    fn unreadable_channel_reports_none_not_zero() {
        let mut exchanges = probe_exchanges(&[0x00, 0x00, 0x01, 0x00, 0x00], &[0, 0, 0, 0, 0, 0]);
        exchanges.push((frame(CMD_GET_TEMP, &[1]), None));
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );
        device.channels().unwrap();

        let values = device.read();

        assert_eq!(values["temp2"], None);
    }

    #[test]
    fn set_duty_converts_percent_to_device_units() {
        let mut exchanges = probe_exchanges(&[0, 0, 0, 0], &[0x01, 0x01, 0x01, 0x01, 0x01, 0x01]);
        exchanges.push((
            frame(CMD_SET_FAN_DUTY, &[1, 50]),
            Some(padded_response(&[])),
        ));
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );
        device.channels().unwrap();

        device.set_duty("fan2", 50.0).unwrap();
    }

    #[test]
    fn set_duty_clamps_below_zero() {
        let mut exchanges = probe_exchanges(&[0, 0, 0, 0], &[0x01, 0x01, 0x01, 0x01, 0x01, 0x01]);
        exchanges.push((frame(CMD_SET_FAN_DUTY, &[3, 0]), Some(padded_response(&[]))));
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );
        device.channels().unwrap();

        device.set_duty("fan4", -10.0).unwrap();
    }

    #[test]
    fn set_duty_clamps_above_hundred() {
        let mut exchanges = probe_exchanges(&[0, 0, 0, 0], &[0x01, 0x01, 0x01, 0x01, 0x01, 0x01]);
        exchanges.push((
            frame(CMD_SET_FAN_DUTY, &[2, 100]),
            Some(padded_response(&[])),
        ));
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );
        device.channels().unwrap();

        device.set_duty("fan3", 110.0).unwrap();
    }

    #[test]
    fn set_duty_on_disconnected_fan_is_noop() {
        let exchanges = probe_exchanges(&[0, 0, 0, 0], &[0, 0, 0, 0, 0, 0]);
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );
        device.channels().unwrap();

        device.set_duty("fan2", 50.0).unwrap();
    }

    #[test]
    fn exchange_drains_stale_reports_before_every_write() {
        let mut exchanges = probe_exchanges(&[0, 0, 0, 0], &[0x01, 0x01, 0x01, 0x01, 0x01, 0x01]);
        exchanges.push((
            frame(CMD_SET_FAN_DUTY, &[1, 50]),
            Some(padded_response(&[])),
        ));
        let transport = FakeTransport::new(exchanges);
        let writes = transport.write_counter();
        let drains = transport.drain_counter();
        let mut device =
            CommanderPro::new(Box::new(transport), "commander-pro", ReleaseMode::PinFull);
        device.channels().unwrap();
        device.set_duty("fan2", 50.0).unwrap();

        assert_eq!(
            drains.load(std::sync::atomic::Ordering::Relaxed),
            writes.load(std::sync::atomic::Ordering::Relaxed)
        );
    }

    #[test]
    fn set_duty_rejects_unknown_channel() {
        let exchanges = probe_exchanges(&[0, 0, 0, 0], &[0, 0, 0, 0, 0, 0]);
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );
        device.channels().unwrap();

        assert!(device.set_duty("fan9", 50.0).is_err());
        assert!(device.set_duty("pwm1", 50.0).is_err());
    }

    #[test]
    fn release_sets_full_duty_as_documented_fallback() {
        let mut exchanges = probe_exchanges(&[0, 0, 0, 0], &[0x01, 0x01, 0x01, 0x01, 0x01, 0x01]);
        exchanges.push((
            frame(CMD_SET_FAN_DUTY, &[1, 100]),
            Some(padded_response(&[])),
        ));
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );
        device.channels().unwrap();

        device.release("fan2").unwrap();
    }

    #[test]
    fn release_on_disconnected_fan_is_noop() {
        let exchanges = probe_exchanges(&[0, 0, 0, 0], &[0, 0, 0, 0, 0, 0]);
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::PinFull,
        );
        device.channels().unwrap();

        device.release("fan2").unwrap();
    }

    #[test]
    fn release_with_keep_last_writes_nothing() {
        let exchanges = probe_exchanges(&[0, 0, 0, 0], &[0x01, 0x01, 0x01, 0x01, 0x01, 0x01]);
        let transport = FakeTransport::new(exchanges);
        let writes = transport.write_counter();
        let mut device =
            CommanderPro::new(Box::new(transport), "commander-pro", ReleaseMode::KeepLast);
        device.channels().unwrap();
        let writes_after_probe = writes.load(std::sync::atomic::Ordering::Relaxed);

        device.release("fan2").unwrap();

        assert_eq!(
            writes.load(std::sync::atomic::Ordering::Relaxed),
            writes_after_probe
        );
    }

    #[test]
    fn release_with_fixed_writes_the_configured_percent() {
        let mut exchanges = probe_exchanges(&[0, 0, 0, 0], &[0x01, 0x01, 0x01, 0x01, 0x01, 0x01]);
        exchanges.push((
            frame(CMD_SET_FAN_DUTY, &[1, 50]),
            Some(padded_response(&[])),
        ));
        let mut device = CommanderPro::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-pro",
            ReleaseMode::Fixed(50),
        );
        device.channels().unwrap();

        device.release("fan2").unwrap();
    }
}
