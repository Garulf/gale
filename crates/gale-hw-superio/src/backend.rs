use std::collections::HashMap;
use std::time::Duration;

use gale_hw::{Backend, ControlInfo, HwError, Id, Inventory, SensorInfo, SensorKind};

use crate::detect::{self, Chip, DetectedChip};
use crate::nct677x::{duty_to_raw, Nct677x};
use crate::restore::{self, ParsedHint};
use crate::transport::{self, PortIo};
use crate::SuperIoStatus;

pub const DETECT_LOCK_TIMEOUT: Duration = Duration::from_millis(1000);
pub const READ_LOCK_TIMEOUT: Duration = Duration::from_millis(100);
pub const WRITE_LOCK_TIMEOUT: Duration = Duration::from_millis(500);

struct ChipEntry {
    slug: String,
    detected: DetectedChip,
    driver: Nct677x,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SensorRefKind {
    Temp,
    Fan,
    Duty,
}

struct SensorRef {
    id: Id,
    chip: usize,
    channel: usize,
    kind: SensorRefKind,
}

pub struct SuperIoBackend {
    io: Box<dyn PortIo>,
    chips: Vec<ChipEntry>,
    current_slot: Option<u8>,
    sensors: Vec<SensorRef>,
    controls: HashMap<Id, (usize, usize)>,
}

pub fn assign_slugs(chips: &[Chip]) -> Vec<String> {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    chips
        .iter()
        .map(|chip| {
            let slug = chip.slug();
            let count = counts.entry(slug).or_insert(0);
            let assigned = if *count == 0 {
                slug.to_string()
            } else {
                format!("{slug}-{count}")
            };
            *count += 1;
            assigned
        })
        .collect()
}

fn ensure_slot(current_slot: &mut Option<u8>, io: &mut dyn PortIo, slot: u8) -> Result<(), String> {
    if *current_slot != Some(slot) {
        detect::reselect(io, slot)?;
        *current_slot = Some(slot);
    }
    Ok(())
}

fn io_error(path: &str, message: String) -> HwError {
    HwError::Io {
        path: path.to_string(),
        message,
    }
}

fn mutex_timeout(path: &str) -> HwError {
    io_error(path, "isa bus mutex timeout".to_string())
}

impl SuperIoBackend {
    pub fn new(mut io: Box<dyn PortIo>) -> Result<Self, SuperIoStatus> {
        let mut chips = Vec::new();
        let mut current_slot = None;

        {
            let mut guard = match transport::lock(io.as_mut(), DETECT_LOCK_TIMEOUT) {
                Ok(Some(guard)) => guard,
                Ok(None) => return Err(SuperIoStatus::MutexTimeout),
                Err(message) => return Err(SuperIoStatus::Io(message)),
            };

            let detected = detect::detect_all(&mut *guard).map_err(SuperIoStatus::Io)?;
            let chip_kinds: Vec<Chip> = detected.iter().map(|entry| entry.chip).collect();
            let slugs = assign_slugs(&chip_kinds);

            for (detected_chip, slug) in detected.into_iter().zip(slugs) {
                if let Err(error) = ensure_slot(&mut current_slot, &mut *guard, detected_chip.slot)
                {
                    tracing::warn!(%error, chip = %slug, "failed to reselect super i/o slot");
                    continue;
                }
                match Nct677x::new(&mut *guard, &detected_chip) {
                    Ok(driver) => chips.push(ChipEntry {
                        slug,
                        detected: detected_chip,
                        driver,
                    }),
                    Err(error) => {
                        tracing::warn!(%error, chip = %slug, "chip failed vendor check, skipping");
                    }
                }
            }
        }

        if chips.is_empty() {
            return Err(SuperIoStatus::NoSupportedChip);
        }

        Ok(Self {
            io,
            chips,
            current_slot,
            sensors: Vec::new(),
            controls: HashMap::new(),
        })
    }

    pub fn chips(&self) -> Vec<(String, DetectedChip)> {
        self.chips
            .iter()
            .map(|entry| (entry.slug.clone(), entry.detected))
            .collect()
    }

    pub fn apply_hint(&mut self, hint: &ParsedHint) -> Result<(), HwError> {
        let chip_index = self
            .chips
            .iter()
            .position(|entry| entry.slug == hint.slug)
            .ok_or_else(|| HwError::UnknownId(hint.slug.clone()))?;
        if hint.channel >= self.chips[chip_index].driver.control_count() {
            return Err(HwError::UnknownId(hint.slug.clone()));
        }

        let guard = transport::lock(self.io.as_mut(), WRITE_LOCK_TIMEOUT)
            .map_err(|message| io_error(&hint.slug, message))?;
        let Some(mut guard) = guard else {
            return Err(mutex_timeout(&hint.slug));
        };

        ensure_slot(
            &mut self.current_slot,
            &mut *guard,
            self.chips[chip_index].detected.slot,
        )
        .map_err(|message| io_error(&hint.slug, message))?;

        self.chips[chip_index]
            .driver
            .write_control_raw(&mut *guard, hint.channel, hint.mode, hint.pwm)
            .map_err(|message| io_error(&hint.slug, message))
    }
}

impl Backend for SuperIoBackend {
    fn name(&self) -> &str {
        "superio"
    }

    fn enumerate(&mut self) -> Result<Inventory, HwError> {
        let mut inventory = Inventory::default();
        self.sensors.clear();
        self.controls.clear();

        for (chip_index, entry) in self.chips.iter().enumerate() {
            let slug = &entry.slug;

            for (position, (n, label)) in
                entry.driver.temperature_channels().into_iter().enumerate()
            {
                let id = format!("superio/{slug}/temp{n}");
                inventory.sensors.push(SensorInfo {
                    id: id.clone(),
                    label: label.to_string(),
                    kind: SensorKind::Temp,
                });
                self.sensors.push(SensorRef {
                    id,
                    chip: chip_index,
                    channel: position,
                    kind: SensorRefKind::Temp,
                });
            }

            for channel in 0..entry.driver.fan_count() {
                let n = channel + 1;
                let id = format!("superio/{slug}/fan{n}");
                inventory.sensors.push(SensorInfo {
                    id: id.clone(),
                    label: format!("fan{n}"),
                    kind: SensorKind::Rpm,
                });
                self.sensors.push(SensorRef {
                    id,
                    chip: chip_index,
                    channel,
                    kind: SensorRefKind::Fan,
                });
            }

            for channel in 0..entry.driver.control_count() {
                let n = channel + 1;
                let id = format!("superio/{slug}/pwm{n}");
                let label = format!("{slug} pwm{n}");
                inventory.controls.push(ControlInfo {
                    id: id.clone(),
                    label: label.clone(),
                });
                self.controls.insert(id.clone(), (chip_index, channel));
                inventory.sensors.push(SensorInfo {
                    id: id.clone(),
                    label,
                    kind: SensorKind::Duty,
                });
                self.sensors.push(SensorRef {
                    id,
                    chip: chip_index,
                    channel,
                    kind: SensorRefKind::Duty,
                });
            }
        }

        inventory.sensors.sort_by(|a, b| a.id.cmp(&b.id));
        inventory.controls.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(inventory)
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        let mut values: HashMap<Id, Option<f64>> = self
            .sensors
            .iter()
            .map(|sensor| (sensor.id.clone(), None))
            .collect();

        let lock_result = transport::lock(self.io.as_mut(), READ_LOCK_TIMEOUT);
        let mut guard = match lock_result {
            Ok(Some(guard)) => guard,
            Ok(None) => {
                tracing::warn!("isa bus mutex busy, skipping tick");
                return values;
            }
            Err(error) => {
                tracing::warn!(%error, "isa bus lock failed, skipping tick");
                return values;
            }
        };

        for (chip_index, entry) in self.chips.iter_mut().enumerate() {
            if let Err(error) =
                ensure_slot(&mut self.current_slot, &mut *guard, entry.detected.slot)
            {
                tracing::warn!(%error, chip = %entry.slug, "failed to reselect super i/o slot");
                continue;
            }

            let mut vendor_ok = entry.driver.vendor_ok(&mut *guard).unwrap_or(false);
            if !vendor_ok {
                let _ = detect::unlock_io_space(&mut *guard, entry.detected.slot);
                vendor_ok = entry.driver.vendor_ok(&mut *guard).unwrap_or(false);
            }
            if !vendor_ok {
                tracing::warn!(chip = %entry.slug, "vendor id check failed, skipping chip this tick");
                continue;
            }

            let fans = entry.driver.read_fans(&mut *guard);
            let temps = entry.driver.read_temperatures(&mut *guard);
            let controls = entry.driver.read_controls(&mut *guard);

            for sensor in self
                .sensors
                .iter()
                .filter(|sensor| sensor.chip == chip_index)
            {
                let value = match sensor.kind {
                    SensorRefKind::Temp => temps.get(sensor.channel).copied().flatten(),
                    SensorRefKind::Fan => fans.get(sensor.channel).copied().flatten(),
                    SensorRefKind::Duty => controls.get(sensor.channel).copied().flatten(),
                };
                values.insert(sensor.id.clone(), value);
            }
        }

        values
    }

    fn set_duty(&mut self, id: &str, pct: f64) -> Result<(), HwError> {
        let &(chip_index, channel) = self
            .controls
            .get(id)
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        if !pct.is_finite() {
            return Err(io_error(id, "non-finite duty".to_string()));
        }

        let guard = transport::lock(self.io.as_mut(), WRITE_LOCK_TIMEOUT)
            .map_err(|message| io_error(id, message))?;
        let Some(mut guard) = guard else {
            return Err(mutex_timeout(id));
        };

        ensure_slot(
            &mut self.current_slot,
            &mut *guard,
            self.chips[chip_index].detected.slot,
        )
        .map_err(|message| io_error(id, message))?;

        let raw = duty_to_raw(pct);
        self.chips[chip_index]
            .driver
            .set_control(&mut *guard, channel, raw)
            .map_err(|message| io_error(id, message))
    }

    fn release(&mut self, id: &str) -> Result<(), HwError> {
        let &(chip_index, channel) = self
            .controls
            .get(id)
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;

        let guard = transport::lock(self.io.as_mut(), WRITE_LOCK_TIMEOUT)
            .map_err(|message| io_error(id, message))?;
        let Some(mut guard) = guard else {
            return Err(mutex_timeout(id));
        };

        ensure_slot(
            &mut self.current_slot,
            &mut *guard,
            self.chips[chip_index].detected.slot,
        )
        .map_err(|message| io_error(id, message))?;

        self.chips[chip_index]
            .driver
            .restore_control(&mut *guard, channel)
            .map_err(|message| io_error(id, message))
    }

    fn restore_hint(&self, id: &str) -> Option<(String, String)> {
        let &(chip_index, channel) = self.controls.get(id)?;
        let saved = self.chips[chip_index].driver.saved(channel)?;
        Some((
            restore::HINT_KIND.to_string(),
            restore::format_hint(&self.chips[chip_index].slug, channel, saved),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{Call, FakeChip, FakePortIo};
    use std::sync::{Arc, Mutex};
    use std::time::Duration as StdDuration;

    struct SharedFakePortIo(Arc<Mutex<FakePortIo>>);

    impl PortIo for SharedFakePortIo {
        fn select_slot(&mut self, slot: u8) -> Result<(), String> {
            self.0.lock().unwrap().select_slot(slot)
        }

        fn find_bars(&mut self) -> Result<(), String> {
            self.0.lock().unwrap().find_bars()
        }

        fn pio_inb(&mut self, port: u16) -> Result<u8, String> {
            self.0.lock().unwrap().pio_inb(port)
        }

        fn pio_outb(&mut self, port: u16, value: u8) -> Result<(), String> {
            self.0.lock().unwrap().pio_outb(port, value)
        }

        fn superio_inb(&mut self, reg: u8) -> Result<u8, String> {
            self.0.lock().unwrap().superio_inb(reg)
        }

        fn superio_inw(&mut self, reg: u8) -> Result<u16, String> {
            self.0.lock().unwrap().superio_inw(reg)
        }

        fn superio_outb(&mut self, reg: u8, value: u8) -> Result<(), String> {
            self.0.lock().unwrap().superio_outb(reg, value)
        }

        fn lock(&mut self, timeout: StdDuration) -> Result<bool, String> {
            self.0.lock().unwrap().lock(timeout)
        }

        fn unlock(&mut self) {
            self.0.lock().unwrap().unlock()
        }

        fn sleep(&mut self, duration: StdDuration) {
            self.0.lock().unwrap().sleep(duration)
        }
    }

    fn backend_with_shared_io(fake: FakePortIo) -> (SuperIoBackend, Arc<Mutex<FakePortIo>>) {
        let shared = Arc::new(Mutex::new(fake));
        let backend = SuperIoBackend::new(Box::new(SharedFakePortIo(shared.clone()))).unwrap();
        (backend, shared)
    }

    fn single_nct6798d() -> SuperIoBackend {
        let io = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        SuperIoBackend::new(Box::new(io)).unwrap()
    }

    #[test]
    fn assign_slugs_appends_dash_n_for_duplicates() {
        assert_eq!(
            assign_slugs(&[Chip::Nct6798D, Chip::Nct6798D, Chip::Nct6798D]),
            vec!["nct6798d", "nct6798d-1", "nct6798d-2"]
        );
        assert_eq!(
            assign_slugs(&[Chip::Nct6798D, Chip::Nct6779D]),
            vec!["nct6798d", "nct6779d"]
        );
    }

    #[test]
    fn enumerate_produces_expected_ids_and_labels() {
        let mut backend = single_nct6798d();
        let inventory = backend.enumerate().unwrap();

        let control_ids: Vec<&str> = inventory.controls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            control_ids,
            vec![
                "superio/nct6798d/pwm1",
                "superio/nct6798d/pwm2",
                "superio/nct6798d/pwm3",
                "superio/nct6798d/pwm4",
                "superio/nct6798d/pwm5",
                "superio/nct6798d/pwm6",
                "superio/nct6798d/pwm7",
            ]
        );

        let fan_ids: Vec<&str> = inventory
            .sensors
            .iter()
            .filter(|s| s.kind == SensorKind::Rpm)
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(
            fan_ids,
            vec![
                "superio/nct6798d/fan1",
                "superio/nct6798d/fan2",
                "superio/nct6798d/fan3",
                "superio/nct6798d/fan4",
                "superio/nct6798d/fan5",
                "superio/nct6798d/fan6",
                "superio/nct6798d/fan7",
            ]
        );

        let temp_ids: Vec<&str> = inventory
            .sensors
            .iter()
            .filter(|s| s.kind == SensorKind::Temp)
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(temp_ids.len(), 24);
        assert!(temp_ids.contains(&"superio/nct6798d/temp1"));
        assert!(temp_ids.contains(&"superio/nct6798d/temp24"));
        assert!(!temp_ids.contains(&"superio/nct6798d/temp25"));

        let duty_ids: Vec<&str> = inventory
            .sensors
            .iter()
            .filter(|s| s.kind == SensorKind::Duty)
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(
            duty_ids,
            vec![
                "superio/nct6798d/pwm1",
                "superio/nct6798d/pwm2",
                "superio/nct6798d/pwm3",
                "superio/nct6798d/pwm4",
                "superio/nct6798d/pwm5",
                "superio/nct6798d/pwm6",
                "superio/nct6798d/pwm7",
            ]
        );

        let cputin = inventory
            .sensors
            .iter()
            .find(|s| s.id == "superio/nct6798d/temp2")
            .unwrap();
        assert_eq!(cputin.label, "CPUTIN");

        let pwm1 = inventory
            .controls
            .iter()
            .find(|c| c.id == "superio/nct6798d/pwm1")
            .unwrap();
        assert_eq!(pwm1.label, "nct6798d pwm1");
    }

    #[test]
    fn set_duty_records_lock_the_full_claim_sequence_and_unlock() {
        let mut chip = FakeChip::nct6798d(0x0290);
        chip.hm.insert(0x102, 0x42);
        chip.hm.insert(0x109, 0x7F);
        let mut fake = FakePortIo::new();
        fake.add_chip(0, chip);
        let (mut backend, shared) = backend_with_shared_io(fake);
        backend.enumerate().unwrap();
        shared.lock().unwrap().take_calls();

        assert_eq!(backend.restore_hint("superio/nct6798d/pwm1"), None);
        backend.set_duty("superio/nct6798d/pwm1", 60.0).unwrap();

        let mut expected = vec![Call::Lock];
        expected.extend([
            Call::PioOut(0x295, 0x4E),
            Call::PioOut(0x296, 0x01),
            Call::PioOut(0x295, 0x02),
            Call::PioIn(0x296),
            Call::PioOut(0x295, 0x4E),
            Call::PioOut(0x296, 0x01),
            Call::PioOut(0x295, 0x09),
            Call::PioIn(0x296),
            Call::PioOut(0x295, 0x4E),
            Call::PioOut(0x296, 0x01),
            Call::PioOut(0x295, 0x02),
            Call::PioOut(0x296, 0x02),
            Call::PioOut(0x295, 0x4E),
            Call::PioOut(0x296, 0x01),
            Call::PioOut(0x295, 0x09),
            Call::PioOut(0x296, 0x99),
        ]);
        expected.push(Call::Unlock);
        assert_eq!(shared.lock().unwrap().take_calls(), expected);

        assert_eq!(
            backend.restore_hint("superio/nct6798d/pwm1"),
            Some(("superio".to_string(), "nct6798d:pwm1:42:7f".to_string()))
        );

        backend.release("superio/nct6798d/pwm1").unwrap();
        assert_eq!(
            shared.lock().unwrap().take_calls(),
            vec![
                Call::Lock,
                Call::PioOut(0x295, 0x4E),
                Call::PioOut(0x296, 0x01),
                Call::PioOut(0x295, 0x02),
                Call::PioOut(0x296, 0x42),
                Call::PioOut(0x295, 0x4E),
                Call::PioOut(0x296, 0x01),
                Call::PioOut(0x295, 0x09),
                Call::PioOut(0x296, 0x7F),
                Call::Unlock,
            ]
        );
        assert_eq!(backend.restore_hint("superio/nct6798d/pwm1"), None);
    }

    #[test]
    fn set_duty_records_no_calls_at_all_for_non_finite_duty() {
        let fake = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let (mut backend, shared) = backend_with_shared_io(fake);
        backend.enumerate().unwrap();
        shared.lock().unwrap().take_calls();

        let result = backend.set_duty("superio/nct6798d/pwm1", f64::NAN);

        assert!(matches!(result, Err(HwError::Io { .. })));
        assert_eq!(shared.lock().unwrap().take_calls(), Vec::new());
    }

    #[test]
    fn set_duty_reports_mutex_error_and_only_locks_when_lock_unavailable() {
        let mut fake = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        fake.lock_available = true;
        let (mut backend, shared) = backend_with_shared_io(fake);
        backend.enumerate().unwrap();
        shared.lock().unwrap().lock_available = false;
        shared.lock().unwrap().take_calls();

        let result = backend.set_duty("superio/nct6798d/pwm1", 60.0);
        match result {
            Err(HwError::Io { message, .. }) => assert!(message.contains("mutex")),
            other => panic!("expected a mutex io error, got {other:?}"),
        }
        assert_eq!(shared.lock().unwrap().take_calls(), vec![Call::Lock]);
    }

    #[test]
    fn read_all_reports_none_for_every_sensor_when_lock_is_unavailable() {
        let mut fake = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        fake.lock_available = true;
        let (mut backend, shared) = backend_with_shared_io(fake);
        let inventory = backend.enumerate().unwrap();
        shared.lock().unwrap().lock_available = false;
        shared.lock().unwrap().take_calls();

        let values = backend.read_all();
        for sensor in &inventory.sensors {
            assert_eq!(values.get(&sensor.id), Some(&None));
        }
        assert_eq!(shared.lock().unwrap().take_calls(), vec![Call::Lock]);
    }

    #[test]
    fn set_duty_rejects_unknown_ids() {
        let mut backend = single_nct6798d();
        backend.enumerate().unwrap();
        assert!(matches!(
            backend.set_duty("superio/ghost/pwm1", 10.0),
            Err(HwError::UnknownId(_))
        ));
    }

    #[test]
    fn read_all_reports_fan_temp_and_duty_values_with_no_select_slot() {
        let mut chip = FakeChip::nct6798d(0x0290);
        chip.hm.insert(0x4B0, 0x23);
        chip.hm.insert(0x4B1, 0x05);
        chip.hm.insert(0x075, 0x2D);
        chip.hm.insert(0x076, 0x00);
        chip.hm.insert(0x200, 0x02);
        chip.hm.insert(0x001, 0x80);
        let mut fake = FakePortIo::new();
        fake.add_chip(0, chip);
        let (mut backend, shared) = backend_with_shared_io(fake);
        backend.enumerate().unwrap();
        shared.lock().unwrap().take_calls();

        let values = backend.read_all();
        assert_eq!(values["superio/nct6798d/fan1"], Some(1200.0));
        assert_eq!(values["superio/nct6798d/temp2"], Some(45.0));
        assert_eq!(values["superio/nct6798d/pwm1"], Some(128.0 / 2.55));

        let calls = shared.lock().unwrap().take_calls();
        assert_eq!(calls.first(), Some(&Call::Lock));
        assert_eq!(calls.last(), Some(&Call::Unlock));
        assert_eq!(
            &calls[1..9],
            &[
                Call::PioOut(0x295, 0x4E),
                Call::PioOut(0x296, 0x80),
                Call::PioOut(0x295, 0x4F),
                Call::PioIn(0x296),
                Call::PioOut(0x295, 0x4E),
                Call::PioOut(0x296, 0x00),
                Call::PioOut(0x295, 0x4F),
                Call::PioIn(0x296),
            ]
        );
        assert!(!calls.iter().any(|call| matches!(call, Call::SelectSlot(_))));
    }

    #[test]
    fn two_chips_reselect_slots_around_each_others_reads() {
        let mut fake = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        fake.add_chip(1, FakeChip::nct6779d(0x0A30));
        let (mut backend, shared) = backend_with_shared_io(fake);
        let chips = backend.chips();
        assert_eq!(chips[0].0, "nct6798d");
        assert_eq!(chips[1].0, "nct6779d");
        backend.enumerate().unwrap();
        shared.lock().unwrap().take_calls();

        backend.read_all();
        let calls = shared.lock().unwrap().take_calls();

        let slot0_start = calls
            .iter()
            .position(|call| *call == Call::SelectSlot(0))
            .unwrap();
        assert_eq!(
            &calls[slot0_start..slot0_start + 5],
            &[
                Call::SelectSlot(0),
                Call::PioOut(0x2E, 0x87),
                Call::PioOut(0x2E, 0x87),
                Call::FindBars,
                Call::PioOut(0x2E, 0xAA),
            ]
        );

        let slot1_start = calls
            .iter()
            .position(|call| *call == Call::SelectSlot(1))
            .unwrap();
        assert_eq!(
            &calls[slot1_start..slot1_start + 5],
            &[
                Call::SelectSlot(1),
                Call::PioOut(0x4E, 0x87),
                Call::PioOut(0x4E, 0x87),
                Call::FindBars,
                Call::PioOut(0x4E, 0xAA),
            ]
        );
        assert!(slot0_start < slot1_start);

        assert!(calls[slot1_start + 5..].contains(&Call::PioOut(0x0A35, 0x4E)));
    }

    #[test]
    fn two_nct6798d_chips_get_dash_one_slug() {
        let mut fake = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        fake.add_chip(1, FakeChip::nct6798d(0x0A30));
        let backend = SuperIoBackend::new(Box::new(fake)).unwrap();
        let slugs: Vec<String> = backend.chips().into_iter().map(|(slug, _)| slug).collect();
        assert_eq!(slugs, vec!["nct6798d", "nct6798d-1"]);
    }

    #[test]
    fn vendor_relock_recovers_and_missing_vendor_id_reports_none() {
        let fake = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let (mut backend, shared) = backend_with_shared_io(fake);
        backend.enumerate().unwrap();
        {
            let mut guard = shared.lock().unwrap();
            guard.set_hm(0, 0x804F, 0x00);
            guard.set_cr28(0, 0x10);
            guard.take_calls();
        }

        let values = backend.read_all();
        for value in values.values() {
            assert_eq!(*value, None);
        }
        let calls = shared.lock().unwrap().take_calls();
        let relock_index = calls
            .windows(5)
            .position(|window| {
                window
                    == [
                        Call::PioOut(0x2E, 0x87),
                        Call::PioOut(0x2E, 0x87),
                        Call::SioIn(0x28),
                        Call::SioOut(0x28, 0x00),
                        Call::PioOut(0x2E, 0xAA),
                    ]
            })
            .expect("expected exactly one unlock_io_space sequence");
        assert!(!calls[relock_index + 5..].windows(5).any(|window| {
            window
                == [
                    Call::PioOut(0x2E, 0x87),
                    Call::PioOut(0x2E, 0x87),
                    Call::SioIn(0x28),
                    Call::SioOut(0x28, 0x00),
                    Call::PioOut(0x2E, 0xAA),
                ]
        }));
    }

    #[test]
    fn apply_hint_writes_bytes_without_claiming() {
        let fake = FakePortIo::with_chip(0, FakeChip::nct6798d(0x0290));
        let (mut backend, shared) = backend_with_shared_io(fake);
        backend.enumerate().unwrap();
        shared.lock().unwrap().take_calls();

        let hint = restore::parse_hint("nct6798d:pwm1:42:7f").unwrap();
        backend.apply_hint(&hint).unwrap();

        assert_eq!(
            shared.lock().unwrap().take_calls(),
            vec![
                Call::Lock,
                Call::PioOut(0x295, 0x4E),
                Call::PioOut(0x296, 0x01),
                Call::PioOut(0x295, 0x02),
                Call::PioOut(0x296, 0x42),
                Call::PioOut(0x295, 0x4E),
                Call::PioOut(0x296, 0x01),
                Call::PioOut(0x295, 0x09),
                Call::PioOut(0x296, 0x7F),
                Call::Unlock,
            ]
        );
        assert_eq!(backend.restore_hint("superio/nct6798d/pwm1"), None);
    }

    #[test]
    fn apply_hint_rejects_unknown_slug() {
        let mut backend = single_nct6798d();
        backend.enumerate().unwrap();
        let hint = restore::parse_hint("ghost:pwm1:42:7f").unwrap();
        assert!(matches!(
            backend.apply_hint(&hint),
            Err(HwError::UnknownId(_))
        ));
    }
}
