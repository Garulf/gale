use gale_core::config::GaleConfig;

#[test]
fn packaged_example_config_parses() {
    let contents = include_str!("../../../packaging/config.example.toml");
    let config = GaleConfig::from_toml(contents).unwrap();
    assert_eq!(config.active_profile, "default");
    assert_eq!(config.api.bind, "127.0.0.1:5250");
    let profile = &config.profiles["default"];
    assert!(profile.assignments.is_empty());
    assert!(!profile.curves.is_empty());
}
