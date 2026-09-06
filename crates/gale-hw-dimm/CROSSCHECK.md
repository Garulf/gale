# DIMM SPD thermal sensor cross-check

`src/spd.rs` and the probe in `src/backend.rs` are ported from RAMSPDToolkit (Blacktempel/RAMSPDToolkit) at commit
0ea51855144eef82787f983004fb8ce7346f8227, files `SPD/DDR4Accessor.cs`, `SPD/DDR5Accessor.cs`, `SPD/SPDDetector.cs`, `SPD/Interop/*`, and the
LibreHardwareMonitor glue in `Hardware/Memory/MemoryGroup.cs` at commit 9eb854d1fce459115c96590b8e14c1e501dc0603. The privileged transfer goes
through PawnIO.Modules 0.2.11 `SmbusPIIX4` (`ioctl_piix4_port_sel`, `ioctl_smbus_xfer`).

1. Addresses 0x50..0x57 probed on PIIX4 ports 0 and 1; the previous port is restored after each transaction group: match
   (RAMSPDToolkit loads two module instances instead; the visible bus behaviour is the same).
2. DDR4 page 0 select: write_byte_data(0x36, 0x00, 0xFF) then a 1 ms pause: match.
3. DDR4 memory type byte 0x02 in {12, 14, 16, 17}; thermal sensor present when byte 0x0E bit 7 is set: match.
4. DDR4 thermal sensor at 0x18 | (spd & 7), register 0x05 read as a word with bytes swapped: match.
5. DDR5 device type MR0 = 0x51 and MR1 = 0x18, thermal sensor enabled when MR26 (0x1A) reads 0, temperature word at
   MR49 (0x31): match. RAMSPDToolkit also handles a non-zero MR11 page and write protection; Gale reads MR0/MR1 directly,
   which are page-independent registers.
6. Temperature decode: 13-bit field, sign at 0x1000, 0.0625 degree steps: match.
7. Retries: 12 for temperatures, 5 for detection bytes, 1 ms apart: match.
8. Gale holds `Global\Access_SMBUS.HTP.Method` for the whole detection pass and for each read_all: match.
