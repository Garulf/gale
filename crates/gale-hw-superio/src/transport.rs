use std::time::Duration;

pub trait PortIo: Send {
    fn select_slot(&mut self, slot: u8) -> Result<(), String>;
    fn find_bars(&mut self) -> Result<(), String>;
    fn pio_inb(&mut self, port: u16) -> Result<u8, String>;
    fn pio_outb(&mut self, port: u16, value: u8) -> Result<(), String>;
    fn superio_inb(&mut self, reg: u8) -> Result<u8, String>;
    fn superio_inw(&mut self, reg: u8) -> Result<u16, String>;
    fn superio_outb(&mut self, reg: u8, value: u8) -> Result<(), String>;
    fn lock(&mut self, timeout: Duration) -> Result<bool, String>;
    fn unlock(&mut self);
    fn sleep(&mut self, duration: Duration);
}

pub struct Locked<'a> {
    io: &'a mut dyn PortIo,
}

pub fn lock(io: &mut dyn PortIo, timeout: Duration) -> Result<Option<Locked<'_>>, String> {
    if io.lock(timeout)? {
        Ok(Some(Locked { io }))
    } else {
        Ok(None)
    }
}

impl<'a> std::ops::Deref for Locked<'a> {
    type Target = dyn PortIo + 'a;

    fn deref(&self) -> &Self::Target {
        &*self.io
    }
}

impl<'a> std::ops::DerefMut for Locked<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.io
    }
}

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        self.io.unlock();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    SelectSlot(u8),
    FindBars,
    PioIn(u16),
    PioOut(u16, u8),
    SioIn(u8),
    SioInw(u8),
    SioOut(u8, u8),
    Lock,
    Unlock,
    Sleep(u64),
}

#[cfg(any(test, feature = "testing"))]
pub struct FakeChip {
    pub id: u8,
    pub revision: u8,
    pub base: u16,
    pub active: bool,
    pub cr28: u8,
    pub hm: std::collections::HashMap<u16, u8>,
}

#[cfg(any(test, feature = "testing"))]
impl FakeChip {
    pub fn nct6798d(base: u16) -> Self {
        Self::with_vendor_bytes(0xD4, 0x2B, base, 0x10)
    }

    pub fn nct6779d(base: u16) -> Self {
        Self::with_vendor_bytes(0xC5, 0x62, base, 0x00)
    }

    fn with_vendor_bytes(id: u8, revision: u8, base: u16, cr28: u8) -> Self {
        let mut hm = std::collections::HashMap::new();
        hm.insert(0x804F, 0x5C);
        hm.insert(0x004F, 0xA3);
        Self {
            id,
            revision,
            base,
            active: true,
            cr28,
            hm,
        }
    }

    pub fn with_hm(mut self, address: u16, value: u8) -> Self {
        self.hm.insert(address, value);
        self
    }
}

#[cfg(any(test, feature = "testing"))]
pub struct FakePortIo {
    pub calls: Vec<Call>,
    pub lock_available: bool,
    pub fail_next: Option<String>,
    pub fail_after: Option<(u32, String)>,
    slot: Option<u8>,
    chips: std::collections::HashMap<u8, FakeChip>,
    config_mode: bool,
    pending_entry: bool,
    ldn: u8,
    bars: Vec<u16>,
    addr_reg: u8,
    bank: u8,
}

#[cfg(any(test, feature = "testing"))]
impl Default for FakePortIo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "testing"))]
impl FakePortIo {
    pub fn new() -> Self {
        Self {
            calls: Vec::new(),
            lock_available: true,
            fail_next: None,
            fail_after: None,
            slot: None,
            chips: std::collections::HashMap::new(),
            config_mode: false,
            pending_entry: false,
            ldn: 0,
            bars: Vec::new(),
            addr_reg: 0,
            bank: 0,
        }
    }

    pub fn with_chip(slot: u8, chip: FakeChip) -> Self {
        let mut io = Self::new();
        io.add_chip(slot, chip);
        io
    }

    pub fn add_chip(&mut self, slot: u8, chip: FakeChip) {
        self.chips.insert(slot, chip);
    }

    pub fn hm(&self, slot: u8, address: u16) -> u8 {
        self.chips
            .get(&slot)
            .and_then(|chip| chip.hm.get(&address).copied())
            .unwrap_or(0)
    }

    pub fn cr28(&self, slot: u8) -> u8 {
        self.chips.get(&slot).map(|chip| chip.cr28).unwrap_or(0)
    }

    pub fn set_hm(&mut self, slot: u8, address: u16, value: u8) {
        if let Some(chip) = self.chips.get_mut(&slot) {
            chip.hm.insert(address, value);
        }
    }

    pub fn set_cr28(&mut self, slot: u8, value: u8) {
        if let Some(chip) = self.chips.get_mut(&slot) {
            chip.cr28 = value;
        }
    }

    pub fn take_calls(&mut self) -> Vec<Call> {
        std::mem::take(&mut self.calls)
    }

    pub fn writes(&self) -> Vec<Call> {
        self.calls
            .iter()
            .filter(|call| {
                matches!(
                    call,
                    Call::SelectSlot(_) | Call::FindBars | Call::PioOut(_, _) | Call::SioOut(_, _)
                )
            })
            .cloned()
            .collect()
    }

    fn index_port(&self) -> Option<u16> {
        match self.slot {
            Some(0) => Some(0x2E),
            Some(1) => Some(0x4E),
            _ => None,
        }
    }

    fn current_chip(&self) -> Option<&FakeChip> {
        self.slot.and_then(|slot| self.chips.get(&slot))
    }

    fn current_chip_mut(&mut self) -> Option<&mut FakeChip> {
        let slot = self.slot?;
        self.chips.get_mut(&slot)
    }

    fn current_chip_base(&self) -> Option<u16> {
        self.current_chip().map(|chip| chip.base)
    }

    fn allow_port(&self, port: u16) -> bool {
        if (0xCF8..=0xCFF).contains(&port) {
            return false;
        }
        if let Some(index) = self.index_port() {
            if port == index || port == index + 1 {
                return true;
            }
        }
        if port == 0x25C || port == 0x25D {
            return true;
        }
        self.bars.contains(&(port & 0xFFF8))
    }

    fn superio_inb_raw(&self, reg: u8) -> u8 {
        if !self.config_mode {
            return 0xFF;
        }
        let chip = self.current_chip();
        match reg {
            0x20 => chip.map(|chip| chip.id).unwrap_or(0xFF),
            0x21 => chip.map(|chip| chip.revision).unwrap_or(0xFF),
            0x07 => self.ldn,
            0x30 if self.ldn == 0x0B => chip.map(|chip| chip.active as u8).unwrap_or(0),
            0x60 if self.ldn == 0x0B => chip.map(|chip| (chip.base >> 8) as u8).unwrap_or(0),
            0x61 if self.ldn == 0x0B => chip.map(|chip| (chip.base & 0xFF) as u8).unwrap_or(0),
            0x28 => chip.map(|chip| chip.cr28).unwrap_or(0),
            _ => 0,
        }
    }

    fn take_failure(&mut self) -> Option<String> {
        if let Some(message) = self.fail_next.take() {
            return Some(message);
        }
        match self.fail_after.take() {
            Some((0, message)) => Some(message),
            Some((remaining, message)) => {
                self.fail_after = Some((remaining - 1, message));
                None
            }
            None => None,
        }
    }
}

#[cfg(any(test, feature = "testing"))]
impl PortIo for FakePortIo {
    fn select_slot(&mut self, slot: u8) -> Result<(), String> {
        self.calls.push(Call::SelectSlot(slot));
        if slot != 0 && slot != 1 {
            return Err("STATUS_INVALID_PARAMETER".to_string());
        }
        self.slot = Some(slot);
        self.bars.clear();
        self.config_mode = false;
        self.pending_entry = false;
        self.ldn = 0;
        Ok(())
    }

    fn find_bars(&mut self) -> Result<(), String> {
        self.calls.push(Call::FindBars);
        let Some(slot) = self.slot else {
            return Err("STATUS_DEVICE_NOT_READY".to_string());
        };
        if !self.config_mode {
            return Err("STATUS_NOT_FOUND".to_string());
        }
        let id = self.chips.get(&slot).map(|chip| chip.id).unwrap_or(0xFF);
        if id == 0x00 || id == 0xFF {
            return Err("STATUS_NOT_FOUND".to_string());
        }
        let base = self.chips[&slot].base;
        self.bars.clear();
        if base >= 0x100 {
            self.bars.push(base);
        }
        Ok(())
    }

    fn pio_inb(&mut self, port: u16) -> Result<u8, String> {
        self.calls.push(Call::PioIn(port));
        if let Some(message) = self.take_failure() {
            return Err(message);
        }
        if !self.allow_port(port) {
            return Err("STATUS_ACCESS_DENIED".to_string());
        }
        if let Some(base) = self.current_chip_base() {
            if port == base + 6 {
                return Ok(if self.addr_reg == 0x4E {
                    self.bank
                } else {
                    let key = ((self.bank as u16) << 8) | self.addr_reg as u16;
                    self.hm(self.slot.unwrap_or(0), key)
                });
            }
        }
        Ok(0)
    }

    fn pio_outb(&mut self, port: u16, value: u8) -> Result<(), String> {
        self.calls.push(Call::PioOut(port, value));
        if let Some(message) = self.take_failure() {
            return Err(message);
        }
        if !self.allow_port(port) {
            return Err("STATUS_ACCESS_DENIED".to_string());
        }
        if Some(port) == self.index_port() {
            if value == 0x87 {
                if self.pending_entry {
                    self.config_mode = true;
                    self.pending_entry = false;
                } else {
                    self.pending_entry = true;
                }
            } else {
                self.pending_entry = false;
                if value == 0xAA {
                    self.config_mode = false;
                }
            }
            return Ok(());
        }
        if let Some(base) = self.current_chip_base() {
            if port == base + 5 {
                self.addr_reg = value;
                return Ok(());
            }
            if port == base + 6 {
                if self.addr_reg == 0x4E {
                    self.bank = value;
                } else {
                    let key = ((self.bank as u16) << 8) | self.addr_reg as u16;
                    let slot = self.slot.unwrap_or(0);
                    self.set_hm(slot, key, value);
                }
                return Ok(());
            }
        }
        Ok(())
    }

    fn superio_inb(&mut self, reg: u8) -> Result<u8, String> {
        self.calls.push(Call::SioIn(reg));
        if let Some(message) = self.take_failure() {
            return Err(message);
        }
        Ok(self.superio_inb_raw(reg))
    }

    fn superio_inw(&mut self, reg: u8) -> Result<u16, String> {
        self.calls.push(Call::SioInw(reg));
        if let Some(message) = self.take_failure() {
            return Err(message);
        }
        let high = self.superio_inb_raw(reg);
        let low = self.superio_inb_raw(reg.wrapping_add(1));
        Ok(((high as u16) << 8) | low as u16)
    }

    fn superio_outb(&mut self, reg: u8, value: u8) -> Result<(), String> {
        self.calls.push(Call::SioOut(reg, value));
        if let Some(message) = self.take_failure() {
            return Err(message);
        }
        if !self.config_mode {
            return Ok(());
        }
        match reg {
            0x07 => self.ldn = value,
            0x28 => {
                if let Some(chip) = self.current_chip_mut() {
                    chip.cr28 = value;
                }
            }
            0x30 => {
                if let Some(chip) = self.current_chip_mut() {
                    chip.active = value != 0;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn lock(&mut self, _timeout: Duration) -> Result<bool, String> {
        self.calls.push(Call::Lock);
        Ok(self.lock_available)
    }

    fn unlock(&mut self) {
        self.calls.push(Call::Unlock);
    }

    fn sleep(&mut self, duration: Duration) {
        self.calls.push(Call::Sleep(duration.as_millis() as u64));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enter_config_mode(io: &mut FakePortIo, index: u16) {
        io.pio_outb(index, 0x87).unwrap();
        io.pio_outb(index, 0x87).unwrap();
    }

    #[test]
    fn double_0x87_write_enters_configuration_mode() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        assert_eq!(io.superio_inb(0x20).unwrap(), 0xFF);
        enter_config_mode(&mut io, 0x2E);
        assert_eq!(io.superio_inb(0x20).unwrap(), 0xD4);
    }

    #[test]
    fn single_0x87_followed_by_0xaa_does_not_enter_configuration_mode() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        io.pio_outb(0x2E, 0x87).unwrap();
        io.pio_outb(0x2E, 0xAA).unwrap();
        assert_eq!(io.superio_inb(0x20).unwrap(), 0xFF);
    }

    #[test]
    fn hm_port_denied_before_find_bars() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        assert_eq!(io.pio_inb(0x0296), Err("STATUS_ACCESS_DENIED".to_string()));
    }

    #[test]
    fn hm_port_allowed_after_find_bars() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        enter_config_mode(&mut io, 0x2E);
        io.find_bars().unwrap();
        assert!(io.pio_inb(0x0296).is_ok());
    }

    #[test]
    fn pci_config_address_ports_are_always_denied() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        enter_config_mode(&mut io, 0x2E);
        io.find_bars().unwrap();
        assert_eq!(io.pio_inb(0x0CF8), Err("STATUS_ACCESS_DENIED".to_string()));
        assert_eq!(
            io.pio_outb(0x0CFF, 0),
            Err("STATUS_ACCESS_DENIED".to_string())
        );
    }

    #[test]
    fn selecting_a_new_slot_clears_bars() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        enter_config_mode(&mut io, 0x2E);
        io.find_bars().unwrap();
        io.select_slot(1).unwrap();
        assert_eq!(io.pio_inb(0x0296), Err("STATUS_ACCESS_DENIED".to_string()));
    }

    #[test]
    fn bank_select_writes_land_in_hardware_monitor_map() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        enter_config_mode(&mut io, 0x2E);
        io.find_bars().unwrap();
        io.pio_outb(0x295, 0x4E).unwrap();
        io.pio_outb(0x296, 0x01).unwrap();
        io.pio_outb(0x295, 0x02).unwrap();
        io.pio_outb(0x296, 0x42).unwrap();
        assert_eq!(io.hm(0, 0x102), 0x42);
    }

    #[test]
    fn superio_inw_reads_high_and_low_bytes_as_one_call() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        enter_config_mode(&mut io, 0x2E);
        io.superio_outb(0x07, 0x0B).unwrap();
        io.take_calls();
        let value = io.superio_inw(0x60).unwrap();
        assert_eq!(value, 0x0290);
        assert_eq!(io.calls, vec![Call::SioInw(0x60)]);
    }

    #[test]
    fn lock_unavailable_returns_ok_false() {
        let mut io = FakePortIo::new();
        io.lock_available = false;
        assert_eq!(io.lock(Duration::ZERO), Ok(false));
    }

    #[test]
    fn transport_lock_returns_none_when_unavailable() {
        let mut io = FakePortIo::new();
        io.lock_available = false;
        assert_eq!(
            lock(&mut io, Duration::ZERO).map(|guard| guard.is_none()),
            Ok(true)
        );
    }

    #[test]
    fn transport_lock_guard_unlocks_on_drop() {
        let mut io = FakePortIo::new();
        io.lock_available = true;
        {
            let guard = lock(&mut io, Duration::ZERO).unwrap();
            assert!(guard.is_some());
        }
        assert_eq!(io.calls.last(), Some(&Call::Unlock));
    }

    #[test]
    fn fail_next_fails_exactly_one_call() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        io.fail_next = Some("injected".to_string());
        assert_eq!(io.superio_inb(0x20), Err("injected".to_string()));
        assert!(io.superio_inb(0x20).is_ok());
    }

    #[test]
    fn fail_next_is_scoped_to_port_and_register_calls() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.fail_next = Some("injected".to_string());
        io.select_slot(0).unwrap();
        let _ = io.find_bars();
        assert_eq!(io.superio_inb(0x20), Err("injected".to_string()));
    }

    #[test]
    fn fail_after_lets_a_fixed_number_of_calls_succeed_before_failing() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        io.fail_after = Some((1, "injected".to_string()));
        assert!(io.superio_inb(0x20).is_ok());
        assert_eq!(io.superio_inb(0x21), Err("injected".to_string()));
        assert!(io.superio_inb(0x20).is_ok());
    }

    #[test]
    fn select_slot_rejects_unknown_slots() {
        let mut io = FakePortIo::new();
        assert_eq!(
            io.select_slot(2),
            Err("STATUS_INVALID_PARAMETER".to_string())
        );
    }

    #[test]
    fn writes_filters_to_mutating_calls_in_order() {
        let mut io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x290));
        io.select_slot(0).unwrap();
        enter_config_mode(&mut io, 0x2E);
        io.superio_inb(0x20).unwrap();
        io.find_bars().unwrap();
        io.superio_outb(0x07, 0x0B).unwrap();
        assert_eq!(
            io.writes(),
            vec![
                Call::SelectSlot(0),
                Call::PioOut(0x2E, 0x87),
                Call::PioOut(0x2E, 0x87),
                Call::FindBars,
                Call::SioOut(0x07, 0x0B),
            ]
        );
    }
}
