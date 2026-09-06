# AMD Zen temperature cross-check

`src/decode.rs` is ported from LibreHardwareMonitor `LibreHardwareMonitorLib/Hardware/Cpu/Amd17Cpu.cs`
at commit 9eb854d1fce459115c96590b8e14c1e501dc0603. The privileged read goes through PawnIO.Modules 0.2.11 `AMDFamily17` (`ioctl_read_smn`).

1. THM_TCON_CUR_TMP 0x00059800, CUR_TEMP in bits 31:21, eighths of a degree: match.
2. 49 degree adjustment when RANGE_SEL (bit 19) is set or TJ_SEL (bits 17:16) == 3: match.
3. Tctl offset table (1600X/1700X/1800X -20, Threadripper 19xx/29xx -27, 2700X -10): match, from k10temp.
4. Per-CCD registers 0x00059954 + 4*i (models 0x31, 0x71, 0x21) and 0x00059b08 + 4*i (0x61, 0x44), raw & 0xFFF,
   (raw*125 - 305000)/1000, kept when raw > 0 and value < 125: match.
5. LibreHardwareMonitor holds `Global\Access_PCI` around the SMN reads with a 10 ms wait; Gale does the same and
   reports every sensor as unavailable for that tick on a timeout instead of reading unlocked.
6. Gale exposes only temperatures; power, clocks and voltages from the same file are not ported.
