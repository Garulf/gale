# NCT6798D fan-control cross-check

Ported register tables and algorithms come from LibreHardwareMonitor at commit
8cbda900bb52a6a8f0cfe39d41aa4d48938e5554. The fan-control path, and only that path,
was additionally checked against Linux `drivers/hwmon/nct6775-core.c` and
`nct6775-platform.c` at torvalds/linux master commit
89a312991dc6e638a36adc43ccb91dbc25504c04, for the NCT6798D.

1. Mode and PWM registers match exactly: Linux `NCT6775_REG_FAN_MODE = {0x102, 0x202, 0x302, 0x802, 0x902, 0xa02, 0xb02}` and `NCT6775_REG_PWM = {0x109, 0x209, 0x309, 0x809, 0x909, 0xa09, 0xb09}` are used for nct6798.
2. PWM read-back matches for the NCT6798D: Linux `NCT6775_REG_PWM_READ = {0x01, 0x03, 0x11, 0x13, 0x15, 0xa09, 0xb09}` equals LibreHardwareMonitor's NCT6797D/98D/99D `FAN_PWM_OUT_REG`. For chips before NCT6797D LibreHardwareMonitor uses 0x017 and 0x029 for channels 6 and 7 while Linux uses 0xA09/0xB09 for every chip; Gale ports LibreHardwareMonitor's table, and this only matters on NCT6791D through NCT6796D.
3. Manual mode byte differs. LibreHardwareMonitor writes 0x00 to `FAN_CONTROL_MODE_REG`, clearing the whole byte. Linux (`store_pwm_enable`) does a read-modify-write: `reg = (reg & 0x0f) | (mode << 4)` where manual is mode 0, bits 6:4 hold the mode (0 manual, 1 thermal cruise, 2 speed cruise, 3 SmartFan III, 4 SmartFan IV) and the low nibble holds the temperature tolerance. Gale follows Linux and writes `saved.mode & 0x0F` as the manual byte, preserving the tolerance nibble. Restore writes the full saved byte in both implementations, so the end state after release is identical either way.
4. Fan speed: LibreHardwareMonitor reads the 13-bit fan count registers 0x4B0 to 0x4BA and 0x4CC and converts with 1.35e6 / count; Linux reads the 16-bit RPM registers `NCT6779_REG_FAN = {0x4c0, 0x4c2, 0x4c4, 0x4c6, 0x4c8, 0x4ca, 0x4ce}` directly. Both are valid views of the same counter; Gale ports the LibreHardwareMonitor path.
5. Bank select: both use register 0x4E through base+5/base+6 (`NCT6775_REG_BANK`, `ADDR_REG_OFFSET 0`, `DATA_REG_OFFSET 1` relative to `IOREGION_OFFSET 5`). Linux caches the current bank; LibreHardwareMonitor writes it on every access. Gale writes it every access so a bank change by another tool holding the mutex between ticks cannot corrupt a read.
6. Chip id matching: Linux masks the 16-bit id with `SIO_ID_MASK 0xFFF8` (`SIO_NCT6798_ID 0xd428` covers 0xD428 to 0xD42F, which includes 0xD42A and 0xD42B); LibreHardwareMonitor matches exact revisions. Gale keeps exact matching so NCT6796DR (0xD42A) and NCT6798D (0xD42B) stay distinct.
7. Logical device activation: Linux reads `SIO_REG_ENABLE 0x30` and forcibly sets bit 0 if the hardware monitor LDN is inactive; LibreHardwareMonitor does this only for NCT6701D. Gale reads 0x30 and skips the chip with a warning when bit 0 is clear; it never writes 0x30.
8. Configuration exit: Linux writes 0xAA and then locks the configuration register (`outb(0x02, ioreg); outb(0x02, ioreg + 1)`); LibreHardwareMonitor writes only 0xAA. Gale writes only 0xAA.
9. The PWM-versus-DC output mode register (Linux `NCT6776_REG_PWM_MODE`, register 0x04 bit 0 for pwm1) is not touched by LibreHardwareMonitor or Gale.
10. Base address: Linux masks the word with `IOREGION_ALIGNMENT (~7)` and refuses base 0; LibreHardwareMonitor strips a Fintek 0x05 offset and requires the word to be readable twice with a 1 ms pause. Gale requires both reads equal, `base != 0`, `base >= 0x100` (the `LpcIO` module refuses smaller BARs anyway) and `base & 7 == 0`.
11. I/O space lock: both clear bit 0x10 of configuration register 0x28 (`NCT6791_REG_HM_IO_SPACE_LOCK_ENABLE`) on NCT6791D and later. Match.
