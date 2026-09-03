pub const WAIT_OBJECT_0: u32 = 0x0000_0000;
pub const WAIT_ABANDONED: u32 = 0x0000_0080;
pub const WAIT_TIMEOUT: u32 = 0x0000_0102;
pub const WAIT_FAILED: u32 = 0xFFFF_FFFF;

pub fn wait_outcome(code: u32) -> Result<bool, String> {
    match code {
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err("WaitForSingleObject failed".to_string()),
        other => Err(format!("unexpected wait result 0x{other:x}")),
    }
}

pub fn hresult_failed(hr: i32) -> bool {
    hr < 0
}

pub fn hresult_message(call: &str, hr: i32) -> String {
    format!("{call} failed with HRESULT 0x{:08x}", hr as u32)
}

pub fn duration_to_wait_ms(d: std::time::Duration) -> u32 {
    let ms = d.as_millis();
    if ms >= u32::MAX as u128 {
        u32::MAX - 1
    } else {
        ms as u32
    }
}

pub const IOCTLS: &[(&str, usize, usize)] = &[
    ("ioctl_select_slot", 1, 0),
    ("ioctl_find_bars", 0, 0),
    ("ioctl_pio_inb", 1, 1),
    ("ioctl_pio_outb", 2, 0),
    ("ioctl_superio_inb", 1, 1),
    ("ioctl_superio_inw", 1, 1),
    ("ioctl_superio_outb", 2, 0),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_outcome_object_0_is_true() {
        assert_eq!(wait_outcome(WAIT_OBJECT_0), Ok(true));
    }

    #[test]
    fn wait_outcome_abandoned_is_true() {
        assert_eq!(wait_outcome(WAIT_ABANDONED), Ok(true));
    }

    #[test]
    fn wait_outcome_timeout_is_false() {
        assert_eq!(wait_outcome(WAIT_TIMEOUT), Ok(false));
    }

    #[test]
    fn wait_outcome_failed_is_err() {
        assert_eq!(
            wait_outcome(WAIT_FAILED),
            Err("WaitForSingleObject failed".to_string())
        );
    }

    #[test]
    fn wait_outcome_unexpected_code_is_err() {
        assert_eq!(
            wait_outcome(0x42),
            Err("unexpected wait result 0x42".to_string())
        );
    }

    #[test]
    fn hresult_failed_for_success_is_false() {
        assert!(!hresult_failed(0));
    }

    #[test]
    fn hresult_failed_for_negative_is_true() {
        assert!(hresult_failed(-1));
    }

    #[test]
    fn hresult_message_ends_with_hresult_hex() {
        let message = hresult_message("pawnio_open", -2147024891);
        assert!(message.ends_with("0x80070005"));
    }

    #[test]
    fn duration_to_wait_ms_never_produces_infinite() {
        assert_eq!(
            duration_to_wait_ms(std::time::Duration::from_millis(u64::MAX)),
            u32::MAX - 1
        );
    }

    #[test]
    fn duration_to_wait_ms_passes_through_normal_values() {
        assert_eq!(
            duration_to_wait_ms(std::time::Duration::from_millis(500)),
            500
        );
    }

    #[test]
    fn ioctl_names_fit_fn_name_length() {
        assert_eq!(IOCTLS.len(), 7);
        for (name, _, _) in IOCTLS {
            assert!(name.len() < 32, "{name} is not under 32 bytes");
        }
    }
}
