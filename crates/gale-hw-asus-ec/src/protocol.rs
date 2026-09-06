// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from LibreHardwareMonitor,
// LibreHardwareMonitorLib/Hardware/Motherboard/Lpc/EC/WindowsEmbeddedControllerIO.cs.

use std::time::Duration;

pub const COMMAND_PORT: u8 = 0x66;
pub const DATA_PORT: u8 = 0x62;
pub const RD_EC: u8 = 0x80;
pub const WR_EC: u8 = 0x81;
pub const STATUS_OUTPUT_BUFFER_FULL: u8 = 0x01;
pub const STATUS_INPUT_BUFFER_FULL: u8 = 0x02;
pub const BANK_REGISTER: u8 = 0xFF;
const MAX_RETRIES: usize = 5;
const WAIT_SPINS: usize = 50;
const FAILURES_BEFORE_SKIP: usize = 20;
const POLL: Duration = Duration::from_millis(1);

pub trait EcPorts: Send {
    fn read_port(&mut self, port: u8) -> Result<u8, String>;
    fn write_port(&mut self, port: u8, value: u8) -> Result<(), String>;
    fn lock(&mut self, timeout: Duration) -> Result<bool, String>;
    fn unlock(&mut self);
    fn sleep(&mut self, duration: Duration);
}

pub struct EcReader {
    wait_read_failures: usize,
}

impl Default for EcReader {
    fn default() -> Self {
        Self::new()
    }
}

impl EcReader {
    pub fn new() -> Self {
        Self {
            wait_read_failures: 0,
        }
    }

    fn wait_write(&mut self, io: &mut dyn EcPorts) -> bool {
        for _ in 0..MAX_RETRIES {
            match io.read_port(COMMAND_PORT) {
                Ok(status) if status & STATUS_INPUT_BUFFER_FULL == 0 => return true,
                Ok(_) => io.sleep(POLL),
                Err(_) => return false,
            }
        }
        false
    }

    fn wait_read(&mut self, io: &mut dyn EcPorts) -> bool {
        if self.wait_read_failures > FAILURES_BEFORE_SKIP {
            return true;
        }
        for _ in 0..MAX_RETRIES {
            match io.read_port(COMMAND_PORT) {
                Ok(status) if status & STATUS_OUTPUT_BUFFER_FULL != 0 => {
                    self.wait_read_failures = 0;
                    return true;
                }
                Ok(_) => io.sleep(POLL),
                Err(_) => return false,
            }
        }
        for _ in 0..WAIT_SPINS {
            match io.read_port(COMMAND_PORT) {
                Ok(status) if status & STATUS_INPUT_BUFFER_FULL == 0 => {
                    self.wait_read_failures = 0;
                    return true;
                }
                Ok(_) => io.sleep(POLL),
                Err(_) => return false,
            }
        }
        self.wait_read_failures += 1;
        false
    }

    fn read_byte_once(&mut self, io: &mut dyn EcPorts, register: u8) -> Option<u8> {
        if !self.wait_write(io) {
            return None;
        }
        io.write_port(COMMAND_PORT, RD_EC).ok()?;
        if !self.wait_write(io) {
            return None;
        }
        io.write_port(DATA_PORT, register).ok()?;
        if !(self.wait_write(io) && self.wait_read(io)) {
            return None;
        }
        io.read_port(DATA_PORT).ok()
    }

    fn write_byte_once(&mut self, io: &mut dyn EcPorts, register: u8, value: u8) -> bool {
        if !self.wait_write(io) {
            return false;
        }
        if io.write_port(COMMAND_PORT, WR_EC).is_err() || !self.wait_write(io) {
            return false;
        }
        if io.write_port(DATA_PORT, register).is_err() || !self.wait_write(io) {
            return false;
        }
        io.write_port(DATA_PORT, value).is_ok()
    }

    pub fn read_byte(&mut self, io: &mut dyn EcPorts, register: u8) -> Option<u8> {
        (0..MAX_RETRIES).find_map(|_| self.read_byte_once(io, register))
    }

    pub fn write_byte(&mut self, io: &mut dyn EcPorts, register: u8, value: u8) -> bool {
        (0..MAX_RETRIES).any(|_| self.write_byte_once(io, register, value))
    }

    fn switch_bank(&mut self, io: &mut dyn EcPorts, bank: u8) -> Option<u8> {
        let previous = self.read_byte(io, BANK_REGISTER)?;
        self.write_byte(io, BANK_REGISTER, bank);
        Some(previous)
    }

    pub fn read_registers(&mut self, io: &mut dyn EcPorts, registers: &[u16]) -> Vec<Option<u8>> {
        let mut bank = 0u8;
        let previous = self.switch_bank(io, bank);
        let mut data = Vec::with_capacity(registers.len());
        for register in registers {
            let wanted_bank = (register >> 8) as u8;
            if wanted_bank != bank {
                self.switch_bank(io, wanted_bank);
                bank = wanted_bank;
            }
            data.push(self.read_byte(io, (register & 0xFF) as u8));
        }
        if let Some(previous) = previous {
            self.switch_bank(io, previous);
        }
        data
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    pub struct FakeEc {
        pub registers: HashMap<(u8, u8), u8>,
        pub bank: u8,
        pub pending_command: Option<u8>,
        pub pending_register: Option<u8>,
        pub output: Option<u8>,
        pub writes: Vec<(u8, u8, u8)>,
        pub port_log: Vec<String>,
        pub locked: bool,
    }

    impl EcPorts for FakeEc {
        fn read_port(&mut self, port: u8) -> Result<u8, String> {
            assert!(self.locked, "ec access outside the ec mutex");
            match port {
                COMMAND_PORT => Ok(if self.output.is_some() {
                    STATUS_OUTPUT_BUFFER_FULL
                } else {
                    0
                }),
                DATA_PORT => {
                    self.port_log.push("read data".into());
                    self.output.take().ok_or_else(|| "no data".to_string())
                }
                other => Err(format!("bad port {other:#x}")),
            }
        }

        fn write_port(&mut self, port: u8, value: u8) -> Result<(), String> {
            assert!(self.locked, "ec access outside the ec mutex");
            self.port_log.push(format!("write {port:#x} {value:#x}"));
            match (port, self.pending_command, self.pending_register) {
                (COMMAND_PORT, _, _) => {
                    self.pending_command = Some(value);
                    self.pending_register = None;
                }
                (DATA_PORT, Some(RD_EC), None) => {
                    let value_at = if value == BANK_REGISTER {
                        self.bank
                    } else {
                        self.registers
                            .get(&(self.bank, value))
                            .copied()
                            .unwrap_or(0)
                    };
                    self.output = Some(value_at);
                    self.pending_command = None;
                }
                (DATA_PORT, Some(WR_EC), None) => self.pending_register = Some(value),
                (DATA_PORT, Some(WR_EC), Some(register)) => {
                    if register == BANK_REGISTER {
                        self.bank = value;
                    } else {
                        self.registers.insert((self.bank, register), value);
                    }
                    self.writes.push((self.bank, register, value));
                    self.pending_command = None;
                    self.pending_register = None;
                }
                _ => return Err("unexpected data write".into()),
            }
            Ok(())
        }

        fn lock(&mut self, _timeout: Duration) -> Result<bool, String> {
            self.locked = true;
            Ok(true)
        }

        fn unlock(&mut self) {
            self.locked = false;
        }

        fn sleep(&mut self, _duration: Duration) {}
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeEc;
    use super::*;

    #[test]
    fn read_byte_follows_the_acpi_ec_command_sequence() {
        let mut ec = FakeEc {
            locked: true,
            ..Default::default()
        };
        ec.registers.insert((0, 0x3A), 51);
        let mut reader = EcReader::new();
        assert_eq!(reader.read_byte(&mut ec, 0x3A), Some(51));
        assert_eq!(
            ec.port_log,
            vec!["write 0x66 0x80", "write 0x62 0x3a", "read data"]
        );
    }

    #[test]
    fn read_registers_switches_banks_for_high_addresses_and_restores_the_previous_bank() {
        let mut ec = FakeEc {
            locked: true,
            ..Default::default()
        };
        ec.bank = 2;
        ec.registers.insert((0, 0x3A), 51);
        ec.registers.insert((1, 0x0D), 33);
        let mut reader = EcReader::new();
        let data = reader.read_registers(&mut ec, &[0x003A, 0x010D, 0x003A]);
        assert_eq!(data, vec![Some(51), Some(33), Some(51)]);
        assert_eq!(ec.bank, 2);
        let bank_writes: Vec<u8> = ec
            .writes
            .iter()
            .filter(|(_, r, _)| *r == BANK_REGISTER)
            .map(|(_, _, v)| *v)
            .collect();
        assert_eq!(bank_writes, vec![0, 1, 0, 2]);
    }
}
