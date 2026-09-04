import { test } from 'node:test';
import assert from 'node:assert/strict';
import { toToml } from '../src/lib/toml.js';

test('renders scalars, nested tables, quoted keys and inline arrays', () => {
  const config = {
    tick_interval_ms: 200,
    active_profile: 'default',
    api: { bind: '127.0.0.1:5250', api_key: null },
    profiles: {
      default: {
        curves: { cpu: { type: 'point', sensor: 'hwmon/x/temp1', points: [[30, 20], [70, 100]], hysteresis: null } },
        assignments: { 'hwmon/x/pwm1': 'cpu' },
      },
    },
  };
  assert.equal(
    toToml(config),
    [
      'tick_interval_ms = 200',
      'active_profile = "default"',
      '',
      '[api]',
      'bind = "127.0.0.1:5250"',
      '',
      '[profiles.default.curves.cpu]',
      'type = "point"',
      'sensor = "hwmon/x/temp1"',
      'points = [[30, 20], [70, 100]]',
      '',
      '[profiles.default.assignments]',
      '"hwmon/x/pwm1" = "cpu"',
    ].join('\n')
  );
});

test('renders inline tables for object values inside arrays', () => {
  assert.equal(toToml({ hardware: { corsair: { on_release: { fixed: 40 } } } }), '[hardware.corsair.on_release]\nfixed = 40');
});
