{ pkgs, module }:

pkgs.testers.runNixOSTest {
  name = "gale";

  nodes.machine = {
    imports = [ module ];
    services.gale.enable = true;
    environment.systemPackages = [ pkgs.curl ];
  };

  testScript = ''
    api = "http://127.0.0.1:5250/api"

    machine.wait_for_unit("galed.service")
    machine.wait_for_open_port(5250)

    machine.succeed("gale status")
    machine.succeed(f"curl -sf {api}/status")
    machine.succeed("curl -sf http://127.0.0.1:5250/ | grep -q '<html'")
    machine.succeed("test -e /etc/udev/rules.d/60-gale-corsair.rules")

    with subtest("saving from the API writes the state directory"):
        machine.succeed(
            f"curl -sf {api}/config"
            f" | curl -sf -X PUT -H 'content-type: application/json' --data-binary @- {api}/config"
        )
        machine.succeed("grep -q active_profile /var/lib/gale/config.toml")

    with subtest("config survives a restart"):
        machine.systemctl("restart galed.service")
        machine.wait_for_open_port(5250)
        machine.succeed(f"curl -sf {api}/status")
  '';
}
