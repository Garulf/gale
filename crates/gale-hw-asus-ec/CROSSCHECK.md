# ASUS embedded controller cross-check

`src/protocol.rs` is ported from LibreHardwareMonitor `Hardware/Motherboard/Lpc/EC/WindowsEmbeddedControllerIO.cs` and
`src/boards.rs` is generated from `EmbeddedController.cs` (board list and `_knownSensors`) plus
`Hardware/Motherboard/Identification.cs` (product strings), all at commit 9eb854d1fce459115c96590b8e14c1e501dc0603. The privileged port I/O goes through
PawnIO.Modules 0.2.11 `LpcACPIEC` (`ioctl_pio_read`, `ioctl_pio_write`).

1. ACPI EC ports 0x66 (command/status) and 0x62 (data), RD_EC 0x80, WR_EC 0x81, IBF bit 1, OBF bit 0: match.
2. Wait loops: IBF clear up to 5 tries of 1 ms before each write; OBF set up to 5 tries then the ASUS fallback of IBF
   clear for 50 spins; after 20 consecutive read-wait failures the read wait is skipped: match.
3. Bank register 0xFF: previous bank read first, banks switched upward as registers require, previous bank restored: match.
4. Decode: 1-byte temperatures are signed, 2-byte values are big-endian signed 16-bit unless flagged little-endian,
   `blank` (-40 on T-Sensor and water temperatures) yields no value, factor and offset applied: match.
5. Gale exposes Temperature and Fan sources only; Voltage, Current and Flow are dropped.
6. Board detection reads `BaseBoardManufacturer` and `BaseBoardProduct` from the registry BIOS key instead of WMI.
