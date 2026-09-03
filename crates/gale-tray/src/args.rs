#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Tray,
    Start,
    Stop,
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Ok(Command::Tray),
        [flag] if flag == "--start" => Ok(Command::Start),
        [flag] if flag == "--stop" => Ok(Command::Stop),
        _ => Err("usage: gale-tray [--start | --stop]".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_is_tray() {
        assert_eq!(parse(&[]), Ok(Command::Tray));
    }

    #[test]
    fn start_flag_is_start() {
        let args = vec!["--start".to_string()];
        assert_eq!(parse(&args), Ok(Command::Start));
    }

    #[test]
    fn stop_flag_is_stop() {
        let args = vec!["--stop".to_string()];
        assert_eq!(parse(&args), Ok(Command::Stop));
    }

    #[test]
    fn unknown_args_are_errors() {
        let args = vec!["--bogus".to_string()];
        assert_eq!(
            parse(&args),
            Err("usage: gale-tray [--start | --stop]".to_string())
        );
    }
}
