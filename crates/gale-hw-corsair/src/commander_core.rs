use crate::transport::HidTransport;
use crate::{CorsairDevice, DeviceChannels};
use gale_hw::SensorKind;
use std::collections::HashMap;

const REPORT_LENGTH: usize = 96;
const READ_TIMEOUT_MS: i32 = 1000;
const MAX_READ_ATTEMPTS: usize = 8;

const CMD_WAKE: [u8; 4] = [0x01, 0x03, 0x00, 0x02];
const CMD_SLEEP: [u8; 4] = [0x01, 0x03, 0x00, 0x01];
const CMD_CLOSE_ENDPOINT: [u8; 3] = [0x05, 0x01, 0x00];
const CMD_OPEN_ENDPOINT: [u8; 2] = [0x0d, 0x00];
const CMD_READ_INITIAL: [u8; 3] = [0x08, 0x00, 0x01];
const CMD_READ_MORE: [u8; 3] = [0x08, 0x00, 0x02];
const CMD_READ_FINAL: [u8; 3] = [0x08, 0x00, 0x03];
const CMD_WRITE: [u8; 2] = [0x06, 0x00];
const CMD_WRITE_MORE: [u8; 2] = [0x07, 0x00];

const MODE_GET_SPEEDS: [u8; 2] = [0x17, 0x00];
const MODE_GET_TEMPS: [u8; 2] = [0x21, 0x00];
const MODE_CONNECTED_SPEEDS: [u8; 2] = [0x1a, 0x00];
const MODE_HW_SPEED_MODE: [u8; 2] = [0x60, 0x6d];
const MODE_HW_FIXED_PERCENT: [u8; 2] = [0x61, 0x6d];

const DATA_TYPE_SPEEDS: [u8; 2] = [0x06, 0x00];
const DATA_TYPE_TEMPS: [u8; 2] = [0x10, 0x00];
const DATA_TYPE_CONNECTED_SPEEDS: [u8; 2] = [0x09, 0x00];
const DATA_TYPE_HW_SPEED_MODE: [u8; 2] = [0x03, 0x00];
const DATA_TYPE_HW_FIXED_PERCENT: [u8; 2] = [0x04, 0x00];

const FAN_MODE_FIXED_PERCENT: u8 = 0x00;
const FAN_MODE_CURVE_PERCENT: u8 = 0x02;

const FAN_COUNT: usize = 6;
const SPEED_PORT_CONNECTED: u8 = 0x07;
const TEMP_PORT_CONNECTED: u8 = 0x00;

fn u16le_from(data: &[u8], offset: usize) -> Option<u16> {
    let lo = *data.get(offset)?;
    let hi = *data.get(offset + 1)?;
    Some(u16::from_le_bytes([lo, hi]))
}

pub struct CommanderCore {
    transport: Box<dyn HidTransport>,
    slug: String,
    has_pump: bool,
    speed_ports: Vec<bool>,
    temp_ports: Vec<bool>,
}

impl CommanderCore {
    pub fn new(transport: Box<dyn HidTransport>, slug: impl Into<String>, has_pump: bool) -> Self {
        Self {
            transport,
            slug: slug.into(),
            has_pump,
            speed_ports: Vec::new(),
            temp_ports: Vec::new(),
        }
    }

    fn send_command(&mut self, command: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
        let mut frame = vec![0u8; REPORT_LENGTH];
        frame[0] = 0x08;
        let data_start = 1 + command.len();
        frame[1..data_start].copy_from_slice(command);
        let data_end = (data_start + data.len()).min(REPORT_LENGTH);
        frame[data_start..data_end].copy_from_slice(&data[..data_end - data_start]);
        self.transport.drain();
        self.transport.write(&frame)?;

        for _ in 0..MAX_READ_ATTEMPTS {
            let Some(response) = self.transport.read_timeout(READ_TIMEOUT_MS)? else {
                return Err("no response from device".into());
            };
            if response.first().copied() != Some(0x00) {
                continue;
            }
            if response.get(1).copied() != Some(command[0]) {
                return Err("response does not match command".into());
            }
            return Ok(response);
        }
        Err("device did not return a ready response".into())
    }

    fn read_data(&mut self, mode: &[u8; 2], data_type: &[u8; 2]) -> Result<Vec<u8>, String> {
        self.send_command(&CMD_OPEN_ENDPOINT, mode)?;
        let initial = self.send_command(&CMD_READ_INITIAL, &[]);
        let more = self.send_command(&CMD_READ_MORE, &[]);
        let final_part = self.send_command(&CMD_READ_FINAL, &[]);
        self.send_command(&CMD_CLOSE_ENDPOINT, &[])?;

        let initial = initial?;
        let more = more?;
        let final_part = final_part?;

        if initial.get(3..5) != Some(data_type.as_slice()) {
            return Err("device returned incorrect data type".into());
        }

        let mut out = initial.get(5..).unwrap_or_default().to_vec();
        out.extend_from_slice(more.get(3..).unwrap_or_default());
        out.extend_from_slice(final_part.get(3..).unwrap_or_default());
        Ok(out)
    }

    fn write_data(
        &mut self,
        mode: &[u8; 2],
        data_type: &[u8; 2],
        data: &[u8],
    ) -> Result<(), String> {
        self.read_data(mode, data_type)?;
        self.send_command(&CMD_OPEN_ENDPOINT, mode)?;

        let mut sent = 0usize;
        while sent < data.len() {
            if sent == 0 {
                let chunk = (REPORT_LENGTH - 9).min(data.len());
                let mut buf = vec![0u8; 4 + data_type.len() + chunk];
                let declared = (data.len() + data_type.len()) as u16;
                buf[0..2].copy_from_slice(&declared.to_le_bytes());
                buf[4..4 + data_type.len()].copy_from_slice(data_type);
                buf[4 + data_type.len()..].copy_from_slice(&data[..chunk]);
                self.send_command(&CMD_WRITE, &buf)?;
                sent += chunk;
            } else {
                let chunk = (REPORT_LENGTH - 3).min(data.len() - sent);
                let payload = data[sent..sent + chunk].to_vec();
                self.send_command(&CMD_WRITE_MORE, &payload)?;
                sent += chunk;
            }
        }

        self.send_command(&CMD_CLOSE_ENDPOINT, &[])?;
        Ok(())
    }

    fn with_wake<T, F>(&mut self, action: F) -> Result<T, String>
    where
        F: FnOnce(&mut Self) -> Result<T, String>,
    {
        let woken = self.send_command(&CMD_WAKE, &[]);
        let result = woken.and_then(|_| action(self));
        let slept = self.send_command(&CMD_SLEEP, &[]);
        result.and_then(|value| slept.map(|_| value))
    }

    fn get_connected_speeds(&mut self) -> Result<Vec<bool>, String> {
        let res = self.read_data(&MODE_CONNECTED_SPEEDS, &DATA_TYPE_CONNECTED_SPEEDS)?;
        let count = res.first().copied().unwrap_or(0) as usize;
        Ok((0..count)
            .map(|i| res.get(i + 1).copied() == Some(SPEED_PORT_CONNECTED))
            .collect())
    }

    fn get_speeds(&mut self) -> Result<Vec<Option<f64>>, String> {
        let res = self.read_data(&MODE_GET_SPEEDS, &DATA_TYPE_SPEEDS)?;
        let count = res.first().copied().unwrap_or(0) as usize;
        let data = res.get(1..).unwrap_or_default();
        Ok((0..count)
            .map(|i| u16le_from(data, i * 2).map(f64::from))
            .collect())
    }

    fn get_temps(&mut self) -> Result<Vec<Option<f64>>, String> {
        let res = self.read_data(&MODE_GET_TEMPS, &DATA_TYPE_TEMPS)?;
        let count = res.first().copied().unwrap_or(0) as usize;
        let data = res.get(1..).unwrap_or_default();
        Ok((0..count)
            .map(|i| {
                if data.get(i * 3).copied() != Some(TEMP_PORT_CONNECTED) {
                    return None;
                }
                u16le_from(data, i * 3 + 1).map(|raw| f64::from(raw) / 10.0)
            })
            .collect())
    }

    fn set_hardware_mode(&mut self, port: usize, mode: u8) -> Result<(), String> {
        let res = self.read_data(&MODE_HW_SPEED_MODE, &DATA_TYPE_HW_SPEED_MODE)?;
        let count = res.first().copied().unwrap_or(0) as usize;
        if port >= count {
            return Err(format!("device has no speed port {port}"));
        }
        let mut data = res
            .get(0..count + 1)
            .ok_or_else(|| "short hardware speed mode response".to_string())?
            .to_vec();
        data[port + 1] = mode;
        self.write_data(&MODE_HW_SPEED_MODE, &DATA_TYPE_HW_SPEED_MODE, &data)
    }

    fn set_fixed_percent(&mut self, port: usize, duty: u16) -> Result<(), String> {
        let res = self.read_data(&MODE_HW_FIXED_PERCENT, &DATA_TYPE_HW_FIXED_PERCENT)?;
        let count = res.first().copied().unwrap_or(0) as usize;
        if port >= count {
            return Err(format!("device has no speed port {port}"));
        }
        let mut data = res
            .get(0..count * 2 + 1)
            .ok_or_else(|| "short fixed percent response".to_string())?
            .to_vec();
        let offset = port * 2 + 1;
        data[offset..offset + 2].copy_from_slice(&duty.to_le_bytes());
        self.write_data(&MODE_HW_FIXED_PERCENT, &DATA_TYPE_HW_FIXED_PERCENT, &data)
    }

    fn parse_channel(&self, channel: &str) -> Result<usize, String> {
        if self.has_pump && channel == "pump" {
            return Ok(0);
        }
        let fan: usize = channel
            .strip_prefix("fan")
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| format!("unknown channel {channel}"))?;
        if !(1..=FAN_COUNT).contains(&fan) {
            return Err(format!("unknown channel {channel}"));
        }
        Ok(if self.has_pump { fan } else { fan - 1 })
    }

    fn speed_channel_id(&self, port: usize) -> String {
        if self.has_pump {
            if port == 0 {
                "pump".to_string()
            } else {
                format!("fan{port}")
            }
        } else {
            format!("fan{}", port + 1)
        }
    }

    fn speed_channel_label(&self, port: usize) -> String {
        if self.has_pump {
            if port == 0 {
                "Pump speed".to_string()
            } else {
                format!("Fan speed {port}")
            }
        } else {
            format!("Fan speed {}", port + 1)
        }
    }

    fn duty_channel_label(&self, port: usize) -> String {
        if self.has_pump {
            if port == 0 {
                "Pump duty".to_string()
            } else {
                format!("Fan {port} duty")
            }
        } else {
            format!("Fan {} duty", port + 1)
        }
    }

    fn temp_channel_label(&self, index: usize) -> String {
        if self.has_pump && index == 0 {
            "Water temperature".to_string()
        } else {
            format!("Temperature {}", index + 1)
        }
    }
}

impl CorsairDevice for CommanderCore {
    fn slug(&self) -> &str {
        &self.slug
    }

    fn channels(&mut self) -> Result<DeviceChannels, String> {
        let (speed_ports, temps) = self.with_wake(|device| {
            let speed_ports = device.get_connected_speeds()?;
            let temps = device.get_temps()?;
            Ok((speed_ports, temps))
        })?;

        self.speed_ports = speed_ports;
        self.temp_ports = temps.iter().map(Option::is_some).collect();

        let mut sensors = Vec::new();
        let mut controls = Vec::new();
        for port in 0..self.speed_ports.len() {
            if !self.speed_ports[port] {
                continue;
            }
            let id = self.speed_channel_id(port);
            sensors.push((id.clone(), SensorKind::Rpm, self.speed_channel_label(port)));
            controls.push((id, self.duty_channel_label(port)));
        }
        for index in 0..self.temp_ports.len() {
            if !self.temp_ports[index] {
                continue;
            }
            sensors.push((
                format!("temp{}", index + 1),
                SensorKind::Temp,
                self.temp_channel_label(index),
            ));
        }

        Ok(DeviceChannels { sensors, controls })
    }

    fn read(&mut self) -> HashMap<String, Option<f64>> {
        let readings = self.with_wake(|device| {
            let speeds = device.get_speeds()?;
            let temps = device.get_temps()?;
            Ok((speeds, temps))
        });

        let (speeds, temps) = readings.unwrap_or_default();

        let mut values = HashMap::new();
        for port in 0..self.speed_ports.len() {
            if !self.speed_ports[port] {
                continue;
            }
            values.insert(
                self.speed_channel_id(port),
                speeds.get(port).copied().flatten(),
            );
        }
        for index in 0..self.temp_ports.len() {
            if !self.temp_ports[index] {
                continue;
            }
            values.insert(
                format!("temp{}", index + 1),
                temps.get(index).copied().flatten(),
            );
        }
        values
    }

    fn set_duty(&mut self, channel: &str, pct: f64) -> Result<(), String> {
        let port = self.parse_channel(channel)?;
        if self.speed_ports.get(port) == Some(&false) {
            return Ok(());
        }
        let duty = pct.clamp(0.0, 100.0).round() as u16;
        self.with_wake(|device| {
            device.set_hardware_mode(port, FAN_MODE_FIXED_PERCENT)?;
            device.set_fixed_percent(port, duty)
        })
    }

    fn release(&mut self, channel: &str) -> Result<(), String> {
        let port = self.parse_channel(channel)?;
        if self.speed_ports.get(port) == Some(&false) {
            return Ok(());
        }
        self.with_wake(|device| device.set_hardware_mode(port, FAN_MODE_CURVE_PERCENT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FakeTransport;

    type Exchange = (Vec<u8>, Option<Vec<u8>>);

    fn frame(command: &[u8], data: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; REPORT_LENGTH];
        buf[0] = 0x08;
        let data_start = 1 + command.len();
        buf[1..data_start].copy_from_slice(command);
        buf[data_start..data_start + data.len()].copy_from_slice(data);
        buf
    }

    fn response(command: u8, tail: &[u8]) -> Option<Vec<u8>> {
        let mut buf = vec![0x00, command, 0x00];
        buf.extend_from_slice(tail);
        Some(buf)
    }

    fn wake() -> Exchange {
        (frame(&CMD_WAKE, &[]), response(CMD_WAKE[0], &[]))
    }

    fn sleep() -> Exchange {
        (frame(&CMD_SLEEP, &[]), response(CMD_SLEEP[0], &[]))
    }

    fn read_exchanges(mode: &[u8; 2], data_type: &[u8; 2], payload: &[u8]) -> Vec<Exchange> {
        let mut initial = data_type.to_vec();
        initial.extend_from_slice(payload);
        vec![
            (
                frame(&CMD_OPEN_ENDPOINT, mode),
                response(CMD_OPEN_ENDPOINT[0], &[]),
            ),
            (frame(&CMD_READ_INITIAL, &[]), response(0x08, &initial)),
            (frame(&CMD_READ_MORE, &[]), response(0x08, &[])),
            (frame(&CMD_READ_FINAL, &[]), response(0x08, &[])),
            (
                frame(&CMD_CLOSE_ENDPOINT, &[]),
                response(CMD_CLOSE_ENDPOINT[0], &[]),
            ),
        ]
    }

    fn write_exchanges(
        mode: &[u8; 2],
        data_type: &[u8; 2],
        payload: &[u8],
        data: &[u8],
    ) -> Vec<Exchange> {
        let mut buf = vec![0u8; 4 + data_type.len() + data.len()];
        let declared = (data.len() + data_type.len()) as u16;
        buf[0..2].copy_from_slice(&declared.to_le_bytes());
        buf[4..6].copy_from_slice(data_type);
        buf[6..].copy_from_slice(data);

        let mut exchanges = read_exchanges(mode, data_type, payload);
        exchanges.push((
            frame(&CMD_OPEN_ENDPOINT, mode),
            response(CMD_OPEN_ENDPOINT[0], &[]),
        ));
        exchanges.push((frame(&CMD_WRITE, &buf), response(CMD_WRITE[0], &[])));
        exchanges.push((
            frame(&CMD_CLOSE_ENDPOINT, &[]),
            response(CMD_CLOSE_ENDPOINT[0], &[]),
        ));
        exchanges
    }

    fn connected_payload(speeds: &[Option<u16>]) -> Vec<u8> {
        let mut payload = vec![speeds.len() as u8];
        for speed in speeds {
            payload.push(if speed.is_some() { 0x07 } else { 0x01 });
        }
        payload
    }

    fn speeds_payload(speeds: &[Option<u16>]) -> Vec<u8> {
        let mut payload = vec![speeds.len() as u8];
        for speed in speeds {
            payload.extend_from_slice(&speed.unwrap_or(0).to_le_bytes());
        }
        payload
    }

    fn temps_payload(temps: &[Option<f64>]) -> Vec<u8> {
        let mut payload = vec![temps.len() as u8];
        for temp in temps {
            match temp {
                Some(value) => {
                    payload.push(0x00);
                    payload.extend_from_slice(&((value * 10.0).round() as u16).to_le_bytes());
                }
                None => {
                    payload.push(0x01);
                    payload.extend_from_slice(&0u16.to_le_bytes());
                }
            }
        }
        payload
    }

    fn probe_exchanges(speeds: &[Option<u16>], temps: &[Option<f64>]) -> Vec<Exchange> {
        let mut exchanges = vec![wake()];
        exchanges.extend(read_exchanges(
            &MODE_CONNECTED_SPEEDS,
            &DATA_TYPE_CONNECTED_SPEEDS,
            &connected_payload(speeds),
        ));
        exchanges.extend(read_exchanges(
            &MODE_GET_TEMPS,
            &DATA_TYPE_TEMPS,
            &temps_payload(temps),
        ));
        exchanges.push(sleep());
        exchanges
    }

    fn status_exchanges(speeds: &[Option<u16>], temps: &[Option<f64>]) -> Vec<Exchange> {
        let mut exchanges = vec![wake()];
        exchanges.extend(read_exchanges(
            &MODE_GET_SPEEDS,
            &DATA_TYPE_SPEEDS,
            &speeds_payload(speeds),
        ));
        exchanges.extend(read_exchanges(
            &MODE_GET_TEMPS,
            &DATA_TYPE_TEMPS,
            &temps_payload(temps),
        ));
        exchanges.push(sleep());
        exchanges
    }

    fn core(exchanges: Vec<Exchange>) -> CommanderCore {
        CommanderCore::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-core",
            true,
        )
    }

    fn core_xt(exchanges: Vec<Exchange>) -> CommanderCore {
        CommanderCore::new(
            Box::new(FakeTransport::new(exchanges)),
            "commander-core-xt",
            false,
        )
    }

    #[test]
    fn channels_expose_pump_and_connected_fans() {
        let speeds = [None, Some(104), None, None, None, None, Some(918)];
        let temps = [None, Some(45.6)];
        let mut device = core(probe_exchanges(&speeds, &temps));

        let channels = device.channels().unwrap();

        let sensor_ids: Vec<_> = channels
            .sensors
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect();
        assert_eq!(sensor_ids, vec!["fan1", "fan6", "temp2"]);
        let control_ids: Vec<_> = channels
            .controls
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(control_ids, vec!["fan1", "fan6"]);
        let control_labels: Vec<_> = channels
            .controls
            .iter()
            .map(|(_, label)| label.as_str())
            .collect();
        assert_eq!(control_labels, vec!["Fan 1 duty", "Fan 6 duty"]);
    }

    #[test]
    fn channels_name_port_zero_pump_when_the_family_has_one() {
        let speeds = [Some(2357), Some(918)];
        let temps = [Some(12.3)];
        let mut device = core(probe_exchanges(&speeds, &temps));

        let channels = device.channels().unwrap();

        let sensors: Vec<_> = channels
            .sensors
            .iter()
            .map(|(id, _, label)| (id.as_str(), label.as_str()))
            .collect();
        assert_eq!(
            sensors,
            vec![
                ("pump", "Pump speed"),
                ("fan1", "Fan speed 1"),
                ("temp1", "Water temperature"),
            ]
        );
    }

    #[test]
    fn channels_on_core_xt_start_fans_at_port_zero() {
        let speeds = [Some(900), Some(1000)];
        let temps = [Some(30.0)];
        let mut device = core_xt(probe_exchanges(&speeds, &temps));

        let channels = device.channels().unwrap();

        let sensors: Vec<_> = channels
            .sensors
            .iter()
            .map(|(id, _, label)| (id.as_str(), label.as_str()))
            .collect();
        assert_eq!(
            sensors,
            vec![
                ("fan1", "Fan speed 1"),
                ("fan2", "Fan speed 2"),
                ("temp1", "Temperature 1"),
            ]
        );
    }

    #[test]
    fn read_reports_speeds_as_rpm_and_temps_in_celsius() {
        let speeds = [
            Some(2357),
            Some(918),
            Some(903),
            Some(501),
            Some(1104),
            Some(1824),
            Some(104),
        ];
        let temps = [Some(12.3), Some(45.6)];
        let mut exchanges = probe_exchanges(&speeds, &temps);
        exchanges.extend(status_exchanges(&speeds, &temps));
        let mut device = core(exchanges);
        device.channels().unwrap();

        let values = device.read();

        assert_eq!(values["pump"], Some(2357.0));
        assert_eq!(values["fan1"], Some(918.0));
        assert_eq!(values["fan6"], Some(104.0));
        assert_eq!(values["temp1"], Some(12.3));
        assert_eq!(values["temp2"], Some(45.6));
    }

    #[test]
    fn read_omits_disconnected_ports() {
        let speeds = [Some(2357), None, Some(903)];
        let temps = [Some(12.3), None];
        let mut exchanges = probe_exchanges(&speeds, &temps);
        exchanges.extend(status_exchanges(&speeds, &temps));
        let mut device = core(exchanges);
        device.channels().unwrap();

        let values = device.read();

        assert_eq!(values.len(), 3);
        assert!(values.contains_key("pump"));
        assert!(values.contains_key("fan2"));
        assert!(values.contains_key("temp1"));
    }

    #[test]
    fn unreadable_device_reports_none_not_zero() {
        let speeds = [Some(2357), Some(918)];
        let temps = [Some(12.3)];
        let mut exchanges = probe_exchanges(&speeds, &temps);
        exchanges.push((frame(&CMD_WAKE, &[]), None));
        exchanges.push((frame(&CMD_SLEEP, &[]), None));
        let mut device = core(exchanges);
        device.channels().unwrap();

        let values = device.read();

        assert_eq!(values["pump"], None);
        assert_eq!(values["fan1"], None);
        assert_eq!(values["temp1"], None);
    }

    #[test]
    fn set_duty_selects_fixed_percent_mode_then_writes_the_duty() {
        let speeds = [
            Some(2357),
            Some(918),
            Some(903),
            Some(501),
            Some(1104),
            Some(1824),
            Some(104),
        ];
        let temps = [Some(12.3)];
        let modes = [1u8, 2, 3, 4, 5, 6, 7];
        let fixed = [8u16, 9, 10, 11, 12, 13, 14];

        let mut mode_payload = vec![modes.len() as u8];
        mode_payload.extend_from_slice(&modes);
        let mut new_modes = mode_payload.clone();
        new_modes[3] = FAN_MODE_FIXED_PERCENT;

        let mut fixed_payload = vec![fixed.len() as u8];
        for value in fixed {
            fixed_payload.extend_from_slice(&value.to_le_bytes());
        }
        let mut new_fixed = fixed_payload.clone();
        new_fixed[5..7].copy_from_slice(&95u16.to_le_bytes());

        let mut exchanges = probe_exchanges(&speeds, &temps);
        exchanges.push(wake());
        exchanges.extend(read_exchanges(
            &MODE_HW_SPEED_MODE,
            &DATA_TYPE_HW_SPEED_MODE,
            &mode_payload,
        ));
        exchanges.extend(write_exchanges(
            &MODE_HW_SPEED_MODE,
            &DATA_TYPE_HW_SPEED_MODE,
            &mode_payload,
            &new_modes,
        ));
        exchanges.extend(read_exchanges(
            &MODE_HW_FIXED_PERCENT,
            &DATA_TYPE_HW_FIXED_PERCENT,
            &fixed_payload,
        ));
        exchanges.extend(write_exchanges(
            &MODE_HW_FIXED_PERCENT,
            &DATA_TYPE_HW_FIXED_PERCENT,
            &fixed_payload,
            &new_fixed,
        ));
        exchanges.push(sleep());

        let mut device = core(exchanges);
        device.channels().unwrap();

        device.set_duty("fan2", 95.0).unwrap();
    }

    fn duty_exchanges(port: usize, duty: u16) -> Vec<Exchange> {
        let modes = [0u8; 7];
        let fixed = [0u16; 7];

        let mut mode_payload = vec![modes.len() as u8];
        mode_payload.extend_from_slice(&modes);
        let mut new_modes = mode_payload.clone();
        new_modes[port + 1] = FAN_MODE_FIXED_PERCENT;

        let mut fixed_payload = vec![fixed.len() as u8];
        for value in fixed {
            fixed_payload.extend_from_slice(&value.to_le_bytes());
        }
        let mut new_fixed = fixed_payload.clone();
        new_fixed[port * 2 + 1..port * 2 + 3].copy_from_slice(&duty.to_le_bytes());

        let mut exchanges = vec![wake()];
        exchanges.extend(read_exchanges(
            &MODE_HW_SPEED_MODE,
            &DATA_TYPE_HW_SPEED_MODE,
            &mode_payload,
        ));
        exchanges.extend(write_exchanges(
            &MODE_HW_SPEED_MODE,
            &DATA_TYPE_HW_SPEED_MODE,
            &mode_payload,
            &new_modes,
        ));
        exchanges.extend(read_exchanges(
            &MODE_HW_FIXED_PERCENT,
            &DATA_TYPE_HW_FIXED_PERCENT,
            &fixed_payload,
        ));
        exchanges.extend(write_exchanges(
            &MODE_HW_FIXED_PERCENT,
            &DATA_TYPE_HW_FIXED_PERCENT,
            &fixed_payload,
            &new_fixed,
        ));
        exchanges.push(sleep());
        exchanges
    }

    #[test]
    fn set_duty_clamps_below_zero() {
        let mut device = core(duty_exchanges(1, 0));
        device.set_duty("fan1", -10.0).unwrap();
    }

    #[test]
    fn set_duty_clamps_above_hundred() {
        let mut device = core(duty_exchanges(0, 100));
        device.set_duty("pump", 140.0).unwrap();
    }

    #[test]
    fn set_duty_targets_the_pump_port_on_aio_devices() {
        let mut device = core(duty_exchanges(0, 40));
        device.set_duty("pump", 40.0).unwrap();
    }

    #[test]
    fn set_duty_shifts_fan_ports_down_on_core_xt() {
        let mut device = core_xt(duty_exchanges(0, 40));
        device.set_duty("fan1", 40.0).unwrap();
    }

    #[test]
    fn set_duty_rejects_unknown_channels() {
        let mut device = core(vec![]);
        assert!(device.set_duty("fan7", 50.0).is_err());
        assert!(device.set_duty("fan", 50.0).is_err());
        assert!(device.set_duty("pwm1", 50.0).is_err());
    }

    #[test]
    fn pump_channel_is_rejected_on_devices_without_a_pump() {
        let mut device = core_xt(vec![]);
        assert!(device.set_duty("pump", 50.0).is_err());
    }

    #[test]
    fn set_duty_on_a_disconnected_port_is_a_noop() {
        let speeds = [Some(2357), None];
        let temps = [Some(12.3)];
        let mut device = core(probe_exchanges(&speeds, &temps));
        device.channels().unwrap();

        device.set_duty("fan1", 50.0).unwrap();
    }

    #[test]
    fn release_restores_the_hardware_curve_profile() {
        let modes = [0u8, 0, 0, 0, 0, 0, 0];
        let mut mode_payload = vec![modes.len() as u8];
        mode_payload.extend_from_slice(&modes);
        let mut new_modes = mode_payload.clone();
        new_modes[3] = FAN_MODE_CURVE_PERCENT;

        let mut exchanges = vec![wake()];
        exchanges.extend(read_exchanges(
            &MODE_HW_SPEED_MODE,
            &DATA_TYPE_HW_SPEED_MODE,
            &mode_payload,
        ));
        exchanges.extend(write_exchanges(
            &MODE_HW_SPEED_MODE,
            &DATA_TYPE_HW_SPEED_MODE,
            &mode_payload,
            &new_modes,
        ));
        exchanges.push(sleep());

        let mut device = core(exchanges);
        device.release("fan2").unwrap();
    }

    #[test]
    fn send_command_drains_stale_reports_before_every_write() {
        let speeds = [Some(2357), Some(918)];
        let temps = [Some(12.3)];
        let exchanges = probe_exchanges(&speeds, &temps);
        let transport = FakeTransport::new(exchanges);
        let writes = transport.write_counter();
        let drains = transport.drain_counter();
        let mut device = CommanderCore::new(Box::new(transport), "commander-core", true);

        device.channels().unwrap();

        assert_eq!(
            drains.load(std::sync::atomic::Ordering::Relaxed),
            writes.load(std::sync::atomic::Ordering::Relaxed)
        );
    }

    #[test]
    fn a_failed_exchange_still_puts_the_device_back_to_sleep() {
        let mut exchanges = vec![wake()];
        exchanges.push((
            frame(&CMD_OPEN_ENDPOINT, &MODE_HW_SPEED_MODE),
            response(CMD_OPEN_ENDPOINT[0], &[]),
        ));
        exchanges.push((
            frame(&CMD_READ_INITIAL, &[]),
            response(0x08, &[0xff, 0xff, 0x07]),
        ));
        exchanges.push((frame(&CMD_READ_MORE, &[]), response(0x08, &[])));
        exchanges.push((frame(&CMD_READ_FINAL, &[]), response(0x08, &[])));
        exchanges.push((
            frame(&CMD_CLOSE_ENDPOINT, &[]),
            response(CMD_CLOSE_ENDPOINT[0], &[]),
        ));
        exchanges.push(sleep());
        let expected_writes = exchanges.len();

        let transport = FakeTransport::new(exchanges);
        let writes = transport.write_counter();
        let mut device = CommanderCore::new(Box::new(transport), "commander-core", true);

        let result = device.release("fan2");

        assert!(result.is_err());
        assert_eq!(
            writes.load(std::sync::atomic::Ordering::Relaxed),
            expected_writes
        );
    }
}
