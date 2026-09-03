// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
// Copyright (C) LibreHardwareMonitor and Contributors.
// Ported to Rust for Gale from LibreHardwareMonitor
// LibreHardwareMonitorLib/Hardware/Motherboard/Lpc/LpcIO.cs, LpcPort.cs
// at commit 8cbda900bb52a6a8f0cfe39d41aa4d48938e5554.

use std::time::Duration;

use crate::transport::PortIo;

pub const SLOTS: [u8; 2] = [0, 1];
pub const INDEX_PORTS: [u16; 2] = [0x2E, 0x4E];
pub const HARDWARE_MONITOR_LDN: u8 = 0x0B;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chip {
    Nct6771F,
    Nct6776F,
    Nct6779D,
    Nct6791D,
    Nct6792D,
    Nct6792DA,
    Nct6793D,
    Nct6795D,
    Nct6796D,
    Nct6796DR,
    Nct6797D,
    Nct6798D,
    Nct6799D,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipId {
    Supported(Chip),
    Unsupported(&'static str),
    Unknown,
}

impl Chip {
    pub fn from_id(id: u8, revision: u8) -> ChipId {
        match id {
            0xB4 if revision & 0xF0 == 0x70 => ChipId::Supported(Chip::Nct6771F),
            0xC3 if revision & 0xF0 == 0x30 => ChipId::Supported(Chip::Nct6776F),
            0xC4 if revision & 0xF0 == 0x50 => ChipId::Unsupported("NCT610XD"),
            0xC5 if revision & 0xF0 == 0x60 => ChipId::Supported(Chip::Nct6779D),
            0xC7 if revision == 0x32 => ChipId::Unsupported("NCT6683D"),
            0xC8 if revision == 0x03 => ChipId::Supported(Chip::Nct6791D),
            0xC9 if revision == 0x11 => ChipId::Supported(Chip::Nct6792D),
            0xC9 if revision == 0x13 => ChipId::Supported(Chip::Nct6792DA),
            0xD1 if revision == 0x21 => ChipId::Supported(Chip::Nct6793D),
            0xD3 if revision == 0x52 => ChipId::Supported(Chip::Nct6795D),
            0xD4 if revision == 0x23 => ChipId::Supported(Chip::Nct6796D),
            0xD4 if revision == 0x2A => ChipId::Supported(Chip::Nct6796DR),
            0xD4 if revision == 0x2B => ChipId::Supported(Chip::Nct6798D),
            0xD4 if revision == 0x51 => ChipId::Supported(Chip::Nct6797D),
            0xD4 if revision == 0x40 || revision == 0x41 => ChipId::Unsupported("NCT6686D"),
            0xD5 if revision == 0x92 => ChipId::Unsupported("NCT6687D"),
            0xD8 if revision == 0x02 => ChipId::Supported(Chip::Nct6799D),
            0xD8 if revision == 0x06 => ChipId::Unsupported("NCT6701D"),
            _ => ChipId::Unknown,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Chip::Nct6771F => "nct6771f",
            Chip::Nct6776F => "nct6776f",
            Chip::Nct6779D => "nct6779d",
            Chip::Nct6791D => "nct6791d",
            Chip::Nct6792D => "nct6792d",
            Chip::Nct6792DA => "nct6792da",
            Chip::Nct6793D => "nct6793d",
            Chip::Nct6795D => "nct6795d",
            Chip::Nct6796D => "nct6796d",
            Chip::Nct6796DR => "nct6796dr",
            Chip::Nct6797D => "nct6797d",
            Chip::Nct6798D => "nct6798d",
            Chip::Nct6799D => "nct6799d",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Chip::Nct6771F => "NCT6771F",
            Chip::Nct6776F => "NCT6776F",
            Chip::Nct6779D => "NCT6779D",
            Chip::Nct6791D => "NCT6791D",
            Chip::Nct6792D => "NCT6792D",
            Chip::Nct6792DA => "NCT6792DA",
            Chip::Nct6793D => "NCT6793D",
            Chip::Nct6795D => "NCT6795D",
            Chip::Nct6796D => "NCT6796D",
            Chip::Nct6796DR => "NCT6796DR",
            Chip::Nct6797D => "NCT6797D",
            Chip::Nct6798D => "NCT6798D",
            Chip::Nct6799D => "NCT6799D",
        }
    }

    pub fn has_io_space_lock(self) -> bool {
        !matches!(self, Chip::Nct6771F | Chip::Nct6776F | Chip::Nct6779D)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectedChip {
    pub chip: Chip,
    pub slot: u8,
    pub revision: u8,
    pub base: u16,
}

fn index_port(slot: u8) -> Result<u16, String> {
    INDEX_PORTS
        .get(slot as usize)
        .copied()
        .ok_or_else(|| format!("invalid super i/o slot {slot}"))
}

pub fn enter_config(io: &mut dyn PortIo, slot: u8) -> Result<(), String> {
    let port = index_port(slot)?;
    io.pio_outb(port, 0x87)?;
    io.pio_outb(port, 0x87)
}

pub fn exit_config(io: &mut dyn PortIo, slot: u8) -> Result<(), String> {
    let port = index_port(slot)?;
    io.pio_outb(port, 0xAA)
}

pub fn detect_slot(io: &mut dyn PortIo, slot: u8) -> Result<Option<DetectedChip>, String> {
    io.select_slot(slot)?;
    enter_config(io, slot)?;

    let result = detect_slot_in_config(io, slot);
    if result.is_err() {
        let _ = exit_config(io, slot);
    }
    result
}

fn detect_slot_in_config(io: &mut dyn PortIo, slot: u8) -> Result<Option<DetectedChip>, String> {
    let id = io.superio_inb(0x20)?;
    let revision = io.superio_inb(0x21)?;

    let chip = match Chip::from_id(id, revision) {
        ChipId::Supported(chip) => chip,
        ChipId::Unknown if id == 0x00 || id == 0xFF => {
            tracing::debug!(
                "no super i/o chip id=0x{id:02x} revision=0x{revision:02x} slot={slot}"
            );
            return Ok(None);
        }
        ChipId::Unknown => {
            let _ = exit_config(io, slot);
            tracing::info!(
                "unknown super i/o chip id=0x{id:02x} revision=0x{revision:02x} slot={slot}"
            );
            return Ok(None);
        }
        ChipId::Unsupported(name) => {
            let _ = exit_config(io, slot);
            tracing::info!("unsupported super i/o chip {name}");
            return Ok(None);
        }
    };

    finish_chip_detection(io, slot, chip, revision)
}

fn finish_chip_detection(
    io: &mut dyn PortIo,
    slot: u8,
    chip: Chip,
    revision: u8,
) -> Result<Option<DetectedChip>, String> {
    io.find_bars()?;
    io.superio_outb(0x07, HARDWARE_MONITOR_LDN)?;

    let active = io.superio_inb(0x30)? & 1;
    if active == 0 {
        let _ = exit_config(io, slot);
        tracing::warn!("hardware monitor logical device inactive");
        return Ok(None);
    }

    let base = io.superio_inw(0x60)?;
    io.sleep(Duration::from_millis(1));
    let verify = io.superio_inw(0x60)?;

    if chip.has_io_space_lock() && base == verify {
        let cr28 = io.superio_inb(0x28)?;
        if cr28 & 0x10 != 0 {
            io.superio_outb(0x28, cr28 & !0x10)?;
        }
    }

    exit_config(io, slot)?;

    if base != verify {
        tracing::warn!("base address verification failed");
        return Ok(None);
    }
    if base == 0 || base < 0x100 || base & 7 != 0 {
        tracing::warn!("invalid hardware monitor base 0x{base:04x}");
        return Ok(None);
    }

    Ok(Some(DetectedChip {
        chip,
        slot,
        revision,
        base,
    }))
}

pub fn detect_all(io: &mut dyn PortIo) -> Result<Vec<DetectedChip>, String> {
    let mut detected = Vec::new();
    for slot in SLOTS {
        if let Some(chip) = detect_slot(io, slot)? {
            detected.push(chip);
        }
    }
    Ok(detected)
}

pub fn reselect(io: &mut dyn PortIo, slot: u8) -> Result<(), String> {
    io.select_slot(slot)?;
    enter_config(io, slot)?;
    io.find_bars()?;
    exit_config(io, slot)
}

pub fn unlock_io_space(io: &mut dyn PortIo, slot: u8) -> Result<(), String> {
    enter_config(io, slot)?;
    let cr28 = io.superio_inb(0x28)?;
    if cr28 & 0x10 != 0 {
        io.superio_outb(0x28, cr28 & !0x10)?;
    }
    exit_config(io, slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{Call, FakeChip, FakePortIo};

    #[test]
    fn from_id_covers_every_row_of_the_chip_table() {
        assert_eq!(Chip::from_id(0xB4, 0x71), ChipId::Supported(Chip::Nct6771F));
        assert_eq!(Chip::from_id(0xB4, 0x70), ChipId::Supported(Chip::Nct6771F));
        assert_eq!(Chip::from_id(0xB4, 0x60), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xC3, 0x33), ChipId::Supported(Chip::Nct6776F));
        assert_eq!(Chip::from_id(0xC3, 0x30), ChipId::Supported(Chip::Nct6776F));
        assert_eq!(Chip::from_id(0xC3, 0x20), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xC4, 0x50), ChipId::Unsupported("NCT610XD"));
        assert_eq!(Chip::from_id(0xC4, 0x5F), ChipId::Unsupported("NCT610XD"));
        assert_eq!(Chip::from_id(0xC4, 0x40), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xC5, 0x62), ChipId::Supported(Chip::Nct6779D));
        assert_eq!(Chip::from_id(0xC5, 0x63), ChipId::Supported(Chip::Nct6779D));
        assert_eq!(Chip::from_id(0xC5, 0x73), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xC7, 0x32), ChipId::Unsupported("NCT6683D"));
        assert_eq!(Chip::from_id(0xC7, 0x33), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xC8, 0x03), ChipId::Supported(Chip::Nct6791D));
        assert_eq!(Chip::from_id(0xC8, 0x04), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xC9, 0x11), ChipId::Supported(Chip::Nct6792D));
        assert_eq!(
            Chip::from_id(0xC9, 0x13),
            ChipId::Supported(Chip::Nct6792DA)
        );
        assert_eq!(Chip::from_id(0xC9, 0x14), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xD1, 0x21), ChipId::Supported(Chip::Nct6793D));
        assert_eq!(Chip::from_id(0xD1, 0x22), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xD3, 0x52), ChipId::Supported(Chip::Nct6795D));
        assert_eq!(Chip::from_id(0xD3, 0x53), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xD4, 0x23), ChipId::Supported(Chip::Nct6796D));
        assert_eq!(
            Chip::from_id(0xD4, 0x2A),
            ChipId::Supported(Chip::Nct6796DR)
        );
        assert_eq!(Chip::from_id(0xD4, 0x2B), ChipId::Supported(Chip::Nct6798D));
        assert_eq!(Chip::from_id(0xD4, 0x51), ChipId::Supported(Chip::Nct6797D));
        assert_eq!(Chip::from_id(0xD4, 0x40), ChipId::Unsupported("NCT6686D"));
        assert_eq!(Chip::from_id(0xD4, 0x41), ChipId::Unsupported("NCT6686D"));
        assert_eq!(Chip::from_id(0xD4, 0x42), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xD5, 0x92), ChipId::Unsupported("NCT6687D"));
        assert_eq!(Chip::from_id(0xD5, 0x93), ChipId::Unknown);

        assert_eq!(Chip::from_id(0xD8, 0x02), ChipId::Supported(Chip::Nct6799D));
        assert_eq!(Chip::from_id(0xD8, 0x06), ChipId::Unsupported("NCT6701D"));
        assert_eq!(Chip::from_id(0xD8, 0x07), ChipId::Unknown);

        assert_eq!(Chip::from_id(0x00, 0x00), ChipId::Unknown);
        assert_eq!(Chip::from_id(0xFF, 0xFF), ChipId::Unknown);
        assert_eq!(Chip::from_id(0x88, 0x50), ChipId::Unknown);
    }

    #[test]
    fn has_io_space_lock_matches_nct6791d_and_later() {
        assert!(!Chip::Nct6771F.has_io_space_lock());
        assert!(!Chip::Nct6776F.has_io_space_lock());
        assert!(!Chip::Nct6779D.has_io_space_lock());
        assert!(Chip::Nct6791D.has_io_space_lock());
        assert!(Chip::Nct6792D.has_io_space_lock());
        assert!(Chip::Nct6792DA.has_io_space_lock());
        assert!(Chip::Nct6793D.has_io_space_lock());
        assert!(Chip::Nct6795D.has_io_space_lock());
        assert!(Chip::Nct6796D.has_io_space_lock());
        assert!(Chip::Nct6796DR.has_io_space_lock());
        assert!(Chip::Nct6797D.has_io_space_lock());
        assert!(Chip::Nct6798D.has_io_space_lock());
        assert!(Chip::Nct6799D.has_io_space_lock());
    }

    #[test]
    fn detect_all_finds_nct6798d_on_slot_zero_with_byte_exact_calls() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let detected = detect_all(&mut io).unwrap();
        assert_eq!(
            detected,
            vec![DetectedChip {
                chip: Chip::Nct6798D,
                slot: 0,
                revision: 0x2B,
                base: 0x0290,
            }]
        );
        assert_eq!(
            io.take_calls(),
            vec![
                Call::SelectSlot(0),
                Call::PioOut(0x2E, 0x87),
                Call::PioOut(0x2E, 0x87),
                Call::SioIn(0x20),
                Call::SioIn(0x21),
                Call::FindBars,
                Call::SioOut(0x07, 0x0B),
                Call::SioIn(0x30),
                Call::SioInw(0x60),
                Call::Sleep(1),
                Call::SioInw(0x60),
                Call::SioIn(0x28),
                Call::SioOut(0x28, 0x00),
                Call::PioOut(0x2E, 0xAA),
                Call::SelectSlot(1),
                Call::PioOut(0x4E, 0x87),
                Call::PioOut(0x4E, 0x87),
                Call::SioIn(0x20),
                Call::SioIn(0x21),
            ]
        );
    }

    #[test]
    fn cr28_lock_bit_is_cleared_when_set() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        detect_all(&mut io).unwrap();
        assert_eq!(io.cr28(0), 0x00);
        assert!(io.calls.contains(&Call::SioOut(0x28, 0x00)));
    }

    #[test]
    fn cr28_already_clear_writes_nothing() {
        let mut chip = FakeChip::nct6798d(0x0290);
        chip.cr28 = 0x00;
        let mut io = FakePortIo::with_chip(0, chip);
        detect_all(&mut io).unwrap();
        let calls = io.take_calls();
        assert!(calls.contains(&Call::SioIn(0x28)));
        assert!(!calls
            .iter()
            .any(|call| matches!(call, Call::SioOut(0x28, _))));
    }

    #[test]
    fn nct6779d_has_no_io_space_lock_register_access() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6779d(0x0A30));
        detect_all(&mut io).unwrap();
        let calls = io.take_calls();
        assert!(!calls.contains(&Call::SioIn(0x28)));
    }

    #[test]
    fn inactive_hardware_monitor_produces_no_chip() {
        let mut chip = FakeChip::nct6798d(0x0290);
        chip.active = false;
        let mut io = FakePortIo::with_chip(0, chip);
        let detected = detect_all(&mut io).unwrap();
        assert!(detected.is_empty());
        let calls = io.take_calls();
        let slot1_start = calls
            .iter()
            .position(|call| *call == Call::SelectSlot(1))
            .unwrap();
        assert_eq!(
            &calls[slot1_start - 2..slot1_start],
            &[Call::SioIn(0x30), Call::PioOut(0x2E, 0xAA)]
        );
        assert!(!calls
            .iter()
            .any(|call| matches!(call, Call::SioOut(0x30, _))));
    }

    #[test]
    fn base_zero_produces_no_chip_with_exit_written() {
        let mut chip = FakeChip::nct6798d(0x0000);
        chip.base = 0x0000;
        let mut io = FakePortIo::with_chip(0, chip);
        let detected = detect_all(&mut io).unwrap();
        assert!(detected.is_empty());
        assert!(io.calls.contains(&Call::PioOut(0x2E, 0xAA)));
    }

    #[test]
    fn base_below_0x100_produces_no_chip_with_exit_written() {
        let chip = FakeChip::nct6798d(0x0080);
        let mut io = FakePortIo::with_chip(0, chip);
        let detected = detect_all(&mut io).unwrap();
        assert!(detected.is_empty());
        assert!(io.calls.contains(&Call::PioOut(0x2E, 0xAA)));
    }

    #[test]
    fn base_not_aligned_to_eight_produces_no_chip_with_exit_written() {
        let chip = FakeChip::nct6798d(0x0293);
        let mut io = FakePortIo::with_chip(0, chip);
        let detected = detect_all(&mut io).unwrap();
        assert!(detected.is_empty());
        assert!(io.calls.contains(&Call::PioOut(0x2E, 0xAA)));
    }

    #[test]
    fn unknown_chip_id_writes_exit_byte_and_returns_no_chip() {
        let mut chip = FakeChip::nct6798d(0x0290);
        chip.id = 0x12;
        chip.revision = 0x34;
        let mut io = FakePortIo::new();
        io.add_chip(0, chip);
        let detected = detect_all(&mut io).unwrap();
        assert!(detected.is_empty());
        assert!(io.calls.contains(&Call::PioOut(0x2E, 0xAA)));
    }

    #[test]
    fn chip_id_0xff_does_not_write_exit_byte() {
        let mut chip = FakeChip::nct6798d(0x0290);
        chip.id = 0xFF;
        chip.revision = 0xFF;
        let mut io = FakePortIo::new();
        io.add_chip(0, chip);
        let detected = detect_all(&mut io).unwrap();
        assert!(detected.is_empty());
        let calls = io.take_calls();
        let slot0_calls = &calls[..calls
            .iter()
            .position(|call| *call == Call::SelectSlot(1))
            .unwrap()];
        assert!(!slot0_calls
            .iter()
            .any(|call| matches!(call, Call::PioOut(_, 0xAA))));
    }

    #[test]
    fn detect_all_finds_two_chips_in_slot_order() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        io.add_chip(1, FakeChip::nct6779d(0x0A30));
        let detected = detect_all(&mut io).unwrap();
        assert_eq!(
            detected,
            vec![
                DetectedChip {
                    chip: Chip::Nct6798D,
                    slot: 0,
                    revision: 0x2B,
                    base: 0x0290,
                },
                DetectedChip {
                    chip: Chip::Nct6779D,
                    slot: 1,
                    revision: 0x62,
                    base: 0x0A30,
                },
            ]
        );
    }

    #[test]
    fn reselect_records_exact_sequence() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        reselect(&mut io, 0).unwrap();
        assert_eq!(
            io.take_calls(),
            vec![
                Call::SelectSlot(0),
                Call::PioOut(0x2E, 0x87),
                Call::PioOut(0x2E, 0x87),
                Call::FindBars,
                Call::PioOut(0x2E, 0xAA),
            ]
        );
    }

    #[test]
    fn unlock_io_space_clears_lock_bit_when_set() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        io.select_slot(0).unwrap();
        io.take_calls();
        unlock_io_space(&mut io, 0).unwrap();
        assert_eq!(
            io.take_calls(),
            vec![
                Call::PioOut(0x2E, 0x87),
                Call::PioOut(0x2E, 0x87),
                Call::SioIn(0x28),
                Call::SioOut(0x28, 0x00),
                Call::PioOut(0x2E, 0xAA),
            ]
        );
    }

    #[test]
    fn unlock_io_space_writes_nothing_when_already_clear() {
        let mut chip = FakeChip::nct6798d(0x0290);
        chip.cr28 = 0x00;
        let mut io = FakePortIo::with_chip(0, chip);
        io.select_slot(0).unwrap();
        io.take_calls();
        unlock_io_space(&mut io, 0).unwrap();
        let calls = io.take_calls();
        assert!(calls.contains(&Call::SioIn(0x28)));
        assert!(!calls
            .iter()
            .any(|call| matches!(call, Call::SioOut(0x28, _))));
    }

    #[test]
    fn transport_error_surfaces_from_detect_all() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        io.fail_next = Some("boom".to_string());
        assert_eq!(detect_all(&mut io), Err("boom".to_string()));
    }

    #[test]
    fn detect_slot_exits_config_mode_when_a_register_access_fails_after_find_bars() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        io.fail_after = Some((4, "boom".to_string()));
        assert_eq!(detect_all(&mut io), Err("boom".to_string()));
        let calls = io.take_calls();
        assert_eq!(calls.last(), Some(&Call::PioOut(0x2E, 0xAA)));
    }
}
