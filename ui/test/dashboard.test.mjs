import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dashboardState, unassignedControls, timedSparklinePath, shortDevice, overview, trendArrow, temperatureUnit, chartCurve, metricSensors, sensorKindFor, connectionNote, formatReceivedTime } from '../src/lib/dashboard.js';

test('shortDevice compacts known vendor prefixes', () => {
  assert.equal(shortDevice('hwmon/nct6798'), 'nct6798');
  assert.equal(shortDevice('corsair/commander-pro-0805'), 'cmd-pro 0805');
  assert.equal(shortDevice('nvidia/0'), 'nvidia gpu 0');
  assert.equal(shortDevice('other/thing'), 'other/thing');
});

test('overview picks the hottest reading, averages duty, and words the headline', () => {
  assert.equal(overview([52.3, null, 55], [46, 35], 0).headline, 'Temperatures are normal');
  assert.equal(overview([52.3, null, 61], [46, 35], 0).headline, 'System is running warm');
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

test('sensorKindFor resolves hardware and virtual sensor kinds', () => {
  const inventory = {
    sensors: [{ id: 'nvidia/0/power', label: 'GPU 0 Power', kind: 'power' }],
    virtual: [{ id: 'virtual/hot', type: 'max', kind: 'temp', inputs: ['t1'] }],
  };
  assert.equal(sensorKindFor('nvidia/0/power', inventory), 'power');
  assert.equal(sensorKindFor('virtual/hot', inventory), 'temp');
  assert.equal(sensorKindFor('missing', inventory), undefined);
  assert.equal(sensorKindFor('', inventory), undefined);
  assert.equal(sensorKindFor('nvidia/0/power', null), undefined);
});

test('dashboardState separates loading, unavailable, loaded and stale', () => {
  const base = { inventory: {}, inventoryError: '', hasSnapshot: true, connected: true };
  assert.equal(dashboardState(base), 'loaded');
  assert.equal(dashboardState({ ...base, connected: false }), 'stale');
  assert.equal(dashboardState({ ...base, hasSnapshot: false }), 'awaiting');
  assert.equal(dashboardState({ ...base, hasSnapshot: false, connected: false }), 'offline');
  assert.equal(dashboardState({ ...base, hasSnapshot: false, connected: false, connectionLost: false }), 'awaiting');
  assert.equal(dashboardState({ ...base, inventory: null }), 'loading');
  assert.equal(dashboardState({ ...base, inventory: null, inventoryError: '503' }), 'unavailable');
});

test('headline makes no health claim while loading or unavailable', () => {
  assert.equal(overview([40], [20], 0, 'loading').headline, 'Reading the system');
  assert.equal(overview([40], [20], 0, 'unavailable').headline, 'Inventory unavailable');
  assert.equal(overview([40], [20], 3, 'loading').warnings, 3);
  assert.equal(overview([40], [20], 0, 'loading').maxTemp, null);
  assert.equal(overview([40], [20], 0, 'unavailable').avgDuty, null);
});

test('headline flags stale data and does not call low duty quiet', () => {
  assert.equal(overview([40], [20], 0, 'stale').headline, 'Last known: temperatures are normal');
  assert.equal(overview([40], [20], 0, 'stale').maxTemp, 40);
  assert.equal(overview([], [20], 0).headline, 'No temperature readings');
  assert.doesNotMatch(overview([40], [10], 0).headline, /quiet/);
});

test('unassignedControls lists controls without a curve', () => {
  const controls = [{ id: 'a' }, { id: 'b' }];
  assert.deepEqual(unassignedControls(controls, { a: 'curve' }).map((c) => c.id), ['b']);
  assert.equal(unassignedControls(controls, undefined).length, 2);
  assert.deepEqual(unassignedControls(null, {}), []);
});

test('timedSparklinePath places samples by age inside the labeled window', () => {
  const samples = [{ t: 590000, v: 1 }, { t: 600000, v: 2 }];
  assert.equal(timedSparklinePath(samples, 600000, 600000, 120, 36), 'M 118.0 34.0 L 120.0 2.0');
  assert.equal(timedSparklinePath([{ t: 0, v: 1 }], 10, 600000, 120, 36), '');
  const old = [{ t: 0, v: 1 }, { t: 100, v: 1 }];
  assert.equal(timedSparklinePath(old, 1200000, 600000, 120, 36), 'M 0.0 18.0 L 0.0 18.0');
});

test('headline tells waiting for a first reading apart from having none and being offline', () => {
  assert.equal(overview([], [], 0, 'awaiting').headline, 'Reading the system');
  assert.equal(overview([], [], 0, 'offline').headline, 'No live readings');
  assert.equal(overview([40], [20], 0, 'offline').maxTemp, null);
});

test('connectionNote keeps the receipt time of the last sample and stays blank when live', () => {
  const at = new Date(2026, 0, 1, 15, 40, 1).getTime();
  assert.equal(formatReceivedTime(null), '');
  assert.equal(formatReceivedTime(at, 'en-US'), '03:40:01 PM');
  assert.equal(connectionNote('stale', at, 'en-US'), 'Live connection lost. Last updated 03:40:01 PM');
  assert.equal(connectionNote('stale', at + 5000, 'en-US'), 'Live connection lost. Last updated 03:40:06 PM');
  assert.equal(connectionNote('stale', null), 'Live connection lost');
  assert.equal(connectionNote('awaiting', null), 'Connected. Waiting for the first reading');
  assert.match(connectionNote('offline', null), /unreachable/);
  assert.equal(connectionNote('loaded', at), '');
});
