{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.gale;
  galed = lib.getExe' cfg.package "galed";
in
{
  options.services.gale = {
    enable = lib.mkEnableOption "the Gale fan control daemon";

    package = lib.mkPackageOption pkgs "gale" { };

    configFile = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/gale/config.toml";
      description = ''
        Path to the daemon's config.toml. The daemon rewrites this file when
        settings are saved from the web UI or API, so it must be writable and
        cannot live in the Nix store. A missing file starts from defaults.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
    services.udev.packages = [ cfg.package ];

    systemd.services.galed = {
      description = "Gale fan control daemon";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      environment.GALE_CONFIG = cfg.configFile;
      serviceConfig = {
        Type = "simple";
        ExecStart = galed;
        ExecStopPost = "${galed} restore";
        Restart = "on-failure";
        RuntimeDirectory = "gale";
        RuntimeDirectoryPreserve = "restart";
        StateDirectory = "gale";
      };
    };
  };
}
