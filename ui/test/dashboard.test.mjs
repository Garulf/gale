import { test } from 'node:test';
import assert from 'node:assert/strict';
import { shortDevice, overview, trendArrow, temperatureUnit } from '../src/lib/dashboard.js';

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
