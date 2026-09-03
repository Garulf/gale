use crate::nct677x::SavedControl;

pub const HINT_KIND: &str = "superio";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHint {
    pub slug: String,
    pub channel: usize,
    pub mode: u8,
    pub pwm: u8,
}

pub fn parse_hint(value: &str) -> Result<ParsedHint, String> {
    let parts: Vec<&str> = value.split(':').collect();
    let [slug, channel_field, mode_field, pwm_field] = parts.as_slice() else {
        return Err(format!("invalid superio hint: {value}"));
    };
    let channel_str = channel_field
        .strip_prefix("pwm")
        .ok_or_else(|| format!("invalid superio hint channel: {value}"))?;
    let channel_number: usize = channel_str
        .parse()
        .map_err(|_| format!("invalid superio hint channel: {value}"))?;
    if channel_number < 1 {
        return Err(format!("invalid superio hint channel: {value}"));
    }
    let mode = u8::from_str_radix(mode_field, 16)
        .map_err(|_| format!("invalid superio hint mode: {value}"))?;
    let pwm = u8::from_str_radix(pwm_field, 16)
        .map_err(|_| format!("invalid superio hint pwm: {value}"))?;
    Ok(ParsedHint {
        slug: slug.to_string(),
        channel: channel_number - 1,
        mode,
        pwm,
    })
}

pub fn format_hint(slug: &str, channel: usize, saved: SavedControl) -> String {
    format!(
        "{slug}:pwm{}:{:02x}:{:02x}",
        channel + 1,
        saved.mode,
        saved.pwm
    )
}

#[cfg(windows)]
pub fn restore_on_hardware(value: &str) -> Result<(), String> {
    let hint = parse_hint(value)?;
    let mut backend = crate::probe().map_err(|status| format!("{status:?}"))?;
    backend.apply_hint(&hint).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hint_reads_slug_channel_mode_and_pwm() {
        assert_eq!(
            parse_hint("nct6798d:pwm1:42:7f"),
            Ok(ParsedHint {
                slug: "nct6798d".to_string(),
                channel: 0,
                mode: 0x42,
                pwm: 0x7F,
            })
        );
    }

    #[test]
    fn parse_hint_handles_suffixed_slugs_and_channel_seven() {
        let parsed = parse_hint("nct6798d-1:pwm7:00:ff").unwrap();
        assert_eq!(parsed.slug, "nct6798d-1");
        assert_eq!(parsed.channel, 6);
        assert_eq!(parsed.mode, 0x00);
        assert_eq!(parsed.pwm, 0xFF);
    }

    #[test]
    fn parse_hint_rejects_pwm0() {
        assert!(parse_hint("nct6798d:pwm0:00:00").is_err());
    }

    #[test]
    fn parse_hint_rejects_wrong_channel_prefix() {
        assert!(parse_hint("nct6798d:fan1:00:00").is_err());
    }

    #[test]
    fn parse_hint_rejects_wrong_field_count() {
        assert!(parse_hint("a:b:c").is_err());
    }

    #[test]
    fn parse_hint_rejects_non_hex_mode() {
        assert!(parse_hint("nct6798d:pwm1:zz:00").is_err());
    }

    #[test]
    fn format_hint_renders_slug_channel_and_hex_bytes() {
        assert_eq!(
            format_hint(
                "nct6798d",
                0,
                SavedControl {
                    mode: 0x42,
                    pwm: 0x7F
                }
            ),
            "nct6798d:pwm1:42:7f"
        );
    }
}
