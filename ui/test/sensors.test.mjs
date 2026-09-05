import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  VIRTUAL_PREFIX,
  SENSOR_TYPES,
  virtualId,
  virtualName,
  isVirtualId,
  sensorOptions,
  defaultVirtualSensor,
  virtualSensorInputs,
  virtualSensorReferences,
  sensorLabel,
  virtualSensorValidationError,
} from '../src/lib/sensors.js';

test('virtualId prefixes the name', () => {
  assert.equal(virtualId('a'), 'virtual/a');
  assert.equal(VIRTUAL_PREFIX, 'virtual/');
});

test('virtualName strips the prefix off a virtual id', () => {
  assert.equal(virtualName('virtual/cpu_hot'), 'cpu_hot');
});

test('isVirtualId recognizes virtual ids', () => {
  assert.equal(isVirtualId('virtual/a'), true);
  assert.equal(isVirtualId('hwmon/x/temp1'), false);
  assert.equal(isVirtualId(null), false);
  assert.equal(isVirtualId(undefined), false);
  assert.equal(isVirtualId(42), false);
});

test('SENSOR_TYPES lists the six virtual sensor types', () => {
  assert.deepEqual(SENSOR_TYPES, ['max', 'min', 'mean', 'offset', 'delta', 'webhook']);
});

test('sensorOptions lists hardware first then virtual, sorted', () => {
  const hardware = [
    { id: 'hwmon/x/temp1', label: 'CPUTIN' },
    { id: 'nvidia/0/temp', label: 'GPU' },
  ];
  const options = sensorOptions(hardware, ['b', 'a'], null);
  assert.deepEqual(
    options.map((o) => o.id),
    ['hwmon/x/temp1', 'nvidia/0/temp', 'virtual/a', 'virtual/b']
  );
  const virtualOptions = options.filter((o) => o.id.startsWith('virtual/'));
  for (const option of virtualOptions) {
    assert.equal(option.label, option.id);
  }
});

test('sensorOptions excludes the editing sensor', () => {
  const hardware = [
    { id: 'hwmon/x/temp1', label: 'CPUTIN' },
    { id: 'nvidia/0/temp', label: 'GPU' },
  ];
  const options = sensorOptions(hardware, ['b', 'a'], 'virtual/a');
  assert.deepEqual(
    options.map((o) => o.id),
    ['hwmon/x/temp1', 'nvidia/0/temp', 'virtual/b']
  );
});

test('sensorOptions treats null hardware as empty', () => {
  const options = sensorOptions(null, ['b', 'a'], null);
  assert.deepEqual(
    options.map((o) => o.id),
    ['virtual/a', 'virtual/b']
  );
});

test('defaultVirtualSensor returns the right shape for each type', () => {
  assert.deepEqual(defaultVirtualSensor('max'), { type: 'max', inputs: [] });
  assert.deepEqual(defaultVirtualSensor('min'), { type: 'min', inputs: [] });
  assert.deepEqual(defaultVirtualSensor('mean'), { type: 'mean', inputs: [], window_s: null });
  assert.deepEqual(defaultVirtualSensor('offset'), {
    type: 'offset',
    input: '',
    add: 0,
    scale: 1,
  });
  assert.deepEqual(defaultVirtualSensor('delta'), { type: 'delta', input: '', window_s: 30 });
  assert.deepEqual(defaultVirtualSensor('webhook'), { type: 'webhook', token: '', timeout_s: null });
});

test('virtualSensorInputs returns inputs array or wraps input', () => {
  assert.deepEqual(virtualSensorInputs({ type: 'max', inputs: ['a', 'b'] }), ['a', 'b']);
  assert.deepEqual(virtualSensorInputs({ type: 'offset', input: 't' }), ['t']);
  assert.deepEqual(virtualSensorInputs({ type: 'webhook', token: 'abc' }), []);
});

test('virtualSensorReferences finds curves and sensors referencing a name', () => {
  const curves = {
    cpu: { type: 'point', sensor: 'virtual/hot' },
    other: { type: 'point', sensor: 'hwmon/x/temp1' },
  };
  const sensors = {
    hot: { type: 'max', inputs: [] },
    smooth: { type: 'mean', inputs: ['virtual/hot'], window_s: null },
    unrelated: { type: 'max', inputs: [] },
  };
  assert.deepEqual(virtualSensorReferences('hot', curves, sensors), [
    'curve "cpu"',
    'sensor "smooth"',
  ]);
  assert.deepEqual(virtualSensorReferences('unrelated', curves, sensors), []);
});

test('virtualSensorValidationError catches empty name', () => {
  assert.equal(
    virtualSensorValidationError('', { type: 'max', inputs: ['a'] }),
    'Sensor "": name must not be empty'
  );
});

test('virtualSensorValidationError catches slash in name', () => {
  assert.equal(
    virtualSensorValidationError('a/b', { type: 'max', inputs: ['a'] }),
    'Sensor "a/b": name must not contain "/"'
  );
});

test('virtualSensorValidationError catches empty inputs for max/min/mean', () => {
  assert.equal(
    virtualSensorValidationError('m', { type: 'max', inputs: [] }),
    'Sensor "m": at least one input is required'
  );
});

test('virtualSensorValidationError catches empty input for offset/delta', () => {
  assert.equal(
    virtualSensorValidationError('o', { type: 'offset', input: '', add: 0, scale: 1 }),
    'Sensor "o": input is required'
  );
});

test('virtualSensorValidationError catches bad add for offset', () => {
  assert.equal(
    virtualSensorValidationError('o', { type: 'offset', input: 't', add: null, scale: 1 }),
    'Sensor "o": add must be a number'
  );
});

test('virtualSensorValidationError catches bad scale for offset', () => {
  assert.equal(
    virtualSensorValidationError('o', { type: 'offset', input: 't', add: 0, scale: null }),
    'Sensor "o": scale must be a number'
  );
});

test('virtualSensorValidationError catches bad window_s for delta', () => {
  assert.equal(
    virtualSensorValidationError('d', { type: 'delta', input: 't', window_s: 0 }),
    'Sensor "d": window_s must be a positive number'
  );
  assert.equal(
    virtualSensorValidationError('d', { type: 'delta', input: 't', window_s: null }),
    'Sensor "d": window_s must be a positive number'
  );
});

test('virtualSensorValidationError allows null window_s for mean', () => {
  assert.equal(
    virtualSensorValidationError('m', { type: 'mean', inputs: ['t'], window_s: null }),
    ''
  );
});

test('virtualSensorValidationError allows a positive window_s for mean', () => {
  assert.equal(
    virtualSensorValidationError('m', { type: 'mean', inputs: ['t'], window_s: 10 }),
    ''
  );
});

test('virtualSensorValidationError allows an absent window_s for mean', () => {
  assert.equal(virtualSensorValidationError('m', { type: 'mean', inputs: ['t'] }), '');
});

test('virtualSensorValidationError checks webhook timeout_s the way it checks mean window_s', () => {
  assert.equal(
    virtualSensorValidationError('w', { type: 'webhook', token: '', timeout_s: 0 }),
    'Sensor "w": timeout_s must be a positive number'
  );
  assert.equal(
    virtualSensorValidationError('w', { type: 'webhook', token: '', timeout_s: -5 }),
    'Sensor "w": timeout_s must be a positive number'
  );
  assert.equal(
    virtualSensorValidationError('w', { type: 'webhook', token: '', timeout_s: null }),
    ''
  );
  assert.equal(virtualSensorValidationError('w', { type: 'webhook', token: '' }), '');
  assert.equal(
    virtualSensorValidationError('w', { type: 'webhook', token: '', timeout_s: 30 }),
    ''
  );
});

test('virtualSensorReferences sees a webhook as a reference source but never a target', () => {
  const sensors = {
    hook: { type: 'webhook', token: 't' },
    agg: { type: 'max', inputs: ['virtual/hook'] },
  };
  assert.deepEqual(virtualSensorReferences('hook', {}, sensors), ['sensor "agg"']);
  assert.deepEqual(virtualSensorReferences('agg', {}, sensors), []);
});

test('sensorLabel resolves virtual names, inventory labels, and falls back to the id', () => {
  const sensors = [{ id: 'hwmon/x/temp1', label: 'CPU' }];
  assert.equal(sensorLabel('virtual/hot', sensors), 'hot');
  assert.equal(sensorLabel('hwmon/x/temp1', sensors), 'CPU');
  assert.equal(sensorLabel('hwmon/x/temp9', sensors), 'hwmon/x/temp9');
  assert.equal(sensorLabel('', sensors), '');
  assert.equal(sensorLabel('hwmon/x/temp1', null), 'hwmon/x/temp1');
});
