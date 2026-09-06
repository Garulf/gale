use std::collections::HashMap;
use std::time::Duration;

use gale_hw::{Backend, HwError, Id, Inventory, SensorInfo, SensorKind};

use crate::boards::{Board, Kind, Source};
use crate::protocol::{EcPorts, EcReader};

pub const EC_LOCK_TIMEOUT: Duration = Duration::from_millis(250);
pub const PREFIX: &str = "ec/asus";

pub struct AsusEcBackend {
    io: Box<dyn EcPorts>,
    reader: EcReader,
    sources: Vec<Source>,
    registers: Vec<u16>,
}

fn id(source: &Source) -> Id {
    format!("{PREFIX}/{}", source.slug)
}

pub fn decode(source: &Source, bytes: &[Option<u8>]) -> Option<f64> {
    let raw = match source.size {
        1 => {
            let byte = bytes.first().copied().flatten()?;
            if source.kind == Kind::Temperature {
                i32::from(byte as i8)
            } else {
                i32::from(byte)
            }
        }
        2 => {
            let first = bytes.first().copied().flatten()?;
            let second = bytes.get(1).copied().flatten()?;
            let (high, low) = if source.little_endian {
                (second, first)
            } else {
                (first, second)
            };
            i32::from(i16::from_be_bytes([high, low]))
        }
        _ => return None,
    };
    if source.blank == Some(raw) || (source.kind == Kind::Temperature && raw == 0) {
        return None;
    }
    Some(f64::from(raw) * source.factor + source.offset)
}

impl AsusEcBackend {
    pub fn new(io: Box<dyn EcPorts>, board: &Board) -> Self {
        let sources = crate::boards::board_sources(board);
        let registers = sources
            .iter()
            .flat_map(|source| {
                (0..u16::from(source.size)).map(move |offset| source.register + offset)
            })
            .collect();
        Self {
            io,
            reader: EcReader::new(),
            sources,
            registers,
        }
    }

    fn read_frame(&mut self) -> Result<Vec<Option<u8>>, String> {
        if !self.io.lock(EC_LOCK_TIMEOUT)? {
            return Err("ec lock timed out".to_string());
        }
        let data = self
            .reader
            .read_registers(self.io.as_mut(), &self.registers);
        self.io.unlock();
        Ok(data)
    }

    fn decode_frame(&self, frame: &[Option<u8>]) -> HashMap<Id, Option<f64>> {
        let mut cursor = 0usize;
        let mut values = HashMap::new();
        for source in &self.sources {
            let size = usize::from(source.size);
            let slice = &frame[cursor..cursor + size];
            values.insert(id(source), decode(source, slice));
            cursor += size;
        }
        values
    }
}

impl Backend for AsusEcBackend {
    fn name(&self) -> &str {
        "asus-ec"
    }

    fn enumerate(&mut self) -> Result<Inventory, HwError> {
        let frame = self.read_frame().map_err(|message| HwError::Io {
            path: PREFIX.to_string(),
            message,
        })?;
        if frame.iter().all(Option::is_none) {
            return Err(HwError::Io {
                path: PREFIX.to_string(),
                message: "embedded controller did not answer".to_string(),
            });
        }
        Ok(Inventory {
            sensors: self
                .sources
                .iter()
                .map(|source| SensorInfo {
                    id: id(source),
                    label: source.name.to_string(),
                    kind: match source.kind {
                        Kind::Temperature => SensorKind::Temp,
                        Kind::Fan => SensorKind::Rpm,
                    },
                })
                .collect(),
            controls: Vec::new(),
        })
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        match self.read_frame() {
            Ok(frame) => self.decode_frame(&frame),
            Err(_) => self
                .sources
                .iter()
                .map(|source| (id(source), None))
                .collect(),
        }
    }

    fn set_duty(&mut self, id: &str, _pct: f64) -> Result<(), HwError> {
        Err(HwError::UnknownId(id.to_string()))
    }

    fn release(&mut self, id: &str) -> Result<(), HwError> {
        Err(HwError::UnknownId(id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::find_board;
    use crate::protocol::fake::FakeEc;

    fn prime_x570() -> FakeEc {
        let mut ec = FakeEc::default();
        ec.registers.insert((0, 0x3A), 51);
        ec.registers.insert((0, 0x3B), 45);
        ec.registers.insert((0, 0x3C), 33);
        ec.registers.insert((0, 0x3D), (-40i8) as u8);
        ec.registers.insert((0, 0x3E), 40);
        ec.registers.insert((0, 0xB4), 0x0B);
        ec.registers.insert((0, 0xB5), 0xB8);
        ec
    }

    #[test]
    fn enumerate_exposes_temperatures_and_fans_with_lhm_names() {
        let board = find_board("PRIME X570-PRO").unwrap();
        let mut backend = AsusEcBackend::new(Box::new(prime_x570()), board);
        let inventory = backend.enumerate().unwrap();
        let ids: Vec<&str> = inventory.sensors.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "ec/asus/chipset",
                "ec/asus/cpu",
                "ec/asus/motherboard",
                "ec/asus/vrm",
                "ec/asus/t_sensor",
                "ec/asus/chipset_fan"
            ]
        );
        assert_eq!(inventory.sensors[5].kind, SensorKind::Rpm);
        assert_eq!(inventory.sensors[5].label, "Chipset Fan");
    }

    #[test]
    fn read_all_decodes_signed_bytes_big_endian_words_and_blanks() {
        let board = find_board("PRIME X570-PRO").unwrap();
        let mut backend = AsusEcBackend::new(Box::new(prime_x570()), board);
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["ec/asus/chipset"], Some(51.0));
        assert_eq!(values["ec/asus/t_sensor"], None);
        assert_eq!(values["ec/asus/vrm"], Some(40.0));
        assert_eq!(values["ec/asus/chipset_fan"], Some(3000.0));
    }

    #[test]
    fn an_exactly_zero_ec_temperature_means_unpopulated() {
        let temp = Source {
            slug: "t",
            name: "T",
            kind: Kind::Temperature,
            register: 0,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        };
        assert_eq!(decode(&temp, &[Some(0)]), None);
        let fan = Source {
            slug: "f",
            name: "F",
            kind: Kind::Fan,
            register: 0,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        };
        assert_eq!(decode(&fan, &[Some(0), Some(0)]), Some(0.0));
    }

    #[test]
    fn decode_handles_negative_temperatures_and_little_endian_words() {
        let temp = Source {
            slug: "t",
            name: "T",
            kind: Kind::Temperature,
            register: 0,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        };
        assert_eq!(decode(&temp, &[Some(0xF6)]), Some(-10.0));
        let fan = Source {
            slug: "f",
            name: "F",
            kind: Kind::Fan,
            register: 0,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: true,
        };
        assert_eq!(decode(&fan, &[Some(0xB8), Some(0x0B)]), Some(3000.0));
        assert_eq!(decode(&fan, &[Some(0xB8), None]), None);
    }
}
