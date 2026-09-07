import { test } from 'node:test';
import assert from 'node:assert/strict';
import { shortDevice, overview, trendArrow, temperatureUnit, chartCurve, metricSensors } from '../src/lib/dashboard.js';

test('shortDevice compacts known vendor prefixes', () => {
  assert.equal(shortDevice('hwmon/nct6798'), 'nct6798');
  assert.equal(shortDevice('corsair/commander-pro-0805'), 'cmd-pro 0805');
  assert.equal(shortDevice('nvidia/0'), 'nvidia gpu 0');
  assert.equal(shortDevice('other/thing'), 'other/thing');
});

test('overview picks the hottest reading, averages duty, and words the headline', () => {
  assert.equal(overview([52.3, null, 61], [46, 35], 0).headline, 'System is cool and quiet');
  assert.equal(overview([85], [40], 0).headline, 'System is running hot');
  assert.equal(overview([50], [80, 90], 0).headline, 'Fans are working hard');
  assert.equal(overview([50], [10], 1).headline, 'One warning needs attention');
  assert.equal(overview([50], [10], 2).headline, '2 warnings need attention');
  const empty = overview([], [], 0);
  assert.equal(empty.maxTemp, null);
  assert.equal(empty.avgDuty, null);
  assert.equal(overview([1, 3], [10, 20], 0).maxTemp, 3);
  assert.equal(overview([1, 3], [10, 20], 0).avgDuty, 15);
});

test('trendArrow ignores tiny drift', () => {
  assert.equal(trendArrow(0.2), '→');
  assert.equal(trendArrow(1), '↗');
  assert.equal(trendArrow(-1), '↘');
});

test('temperatureUnit only treats delta virtual sensors as a rate', () => {
  assert.equal(temperatureUnit('delta'), '°C/min');
  assert.equal(temperatureUnit('max'), '°C');
  assert.equal(temperatureUnit(undefined), '°C');
});

test('chartCurve follows mix, sync and offset nodes to a drawable source curve', () => {
  const curves = {
    cpu: { type: 'point', sensor: 'a', points: [[30, 20], [70, 100]] },
    gpu: { type: 'linear', sensor: 'b', min_temp: 40, max_temp: 80, min_duty: 0, max_duty: 100 },
    fans: { type: 'mix', sources: ['cpu', 'gpu'], mode: 'max' },
    quiet: { type: 'mix', sources: ['cpu', 'gpu'], mode: 'min' },
    follow: { type: 'sync', source: 'fans' },
    boosted: { type: 'offset', source: 'cpu', add: 10, scale: 1 },
    kick: { type: 'trigger', sensor: 'a', on_temp: 60, off_temp: 50, on_duty: 100, off_duty: 20 },
  };
  const values = { a: 50, b: 70 };
  assert.equal(chartCurve('cpu', curves, values).via, '');
  assert.equal(chartCurve('fans', curves, values).id, 'gpu');
  assert.equal(chartCurve('fans', curves, values).via, 'fans');
  assert.equal(chartCurve('quiet', curves, values).id, 'cpu');
  assert.equal(chartCurve('follow', curves, values).id, 'gpu');
  assert.equal(chartCurve('boosted', curves, values).id, 'cpu');
  assert.deepEqual(chartCurve('kick', curves, values).config.points, [[50, 20], [60, 100]]);
  assert.equal(chartCurve('fans', curves, {}).id, 'cpu');
  assert.equal(chartCurve('missing', curves, values), null);
});

import { spinPeriodSeconds } from '../src/lib/dashboard.js';

test('spinPeriodSeconds turns rpm into one visual revolution period, clamped to a readable range', () => {
  assert.equal(spinPeriodSeconds(0), null);
  assert.equal(spinPeriodSeconds(null), null);
  assert.equal(spinPeriodSeconds(600), 2);
  assert.equal(spinPeriodSeconds(1200), 1);
  assert.equal(spinPeriodSeconds(3000), 0.4);
  assert.equal(spinPeriodSeconds(6000), 0.3);
  assert.equal(spinPeriodSeconds(60), 6);
});

test('spinPeriodSeconds falls back to duty when there is no tach', () => {
  assert.equal(spinPeriodSeconds(null, 100), 0.4);
  assert.equal(spinPeriodSeconds(null, 50), 0.8);
  assert.equal(spinPeriodSeconds(null, 0), null);
  assert.equal(spinPeriodSeconds(0, 60), null);
});

test('metricSensors lists non-temperature, non-fan hardware readings with units', () => {
  const inventory = { sensors: [
    { id: 'nvidia/0/temp', label: 'GPU 0 Temp', kind: 'temp' },
    { id: 'nvidia/0/power', label: 'GPU 0 Power', kind: 'power' },
    { id: 'nvidia/0/util', label: 'GPU 0 Usage', kind: 'percent' },
    { id: 'hwmon/x/fan1', label: 'fan1', kind: 'rpm' },
    { id: 'nvidia/0/fan0', label: 'GPU 0 Fan 0', kind: 'duty' },
  ] };
  assert.deepEqual(metricSensors(inventory).map((m) => [m.id, m.unit]), [
    ['nvidia/0/power', 'W'],
    ['nvidia/0/util', '%'],
  ]);
  assert.deepEqual(metricSensors(null), []);
});
