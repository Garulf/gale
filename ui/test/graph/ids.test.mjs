import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  deviceOf,
  sensorNodeId,
  controlNodeId,
  virtualNodeId,
  curveNodeId,
  combineNodeId,
  COMBINE_TYPES,
  isCombineType,
  nodeIdForCurveRef,
  nodeKind,
  nodeName,
  edgeId,
  virtualInputHandles,
} from '../../src/lib/graph/ids.js';

test('deviceOf strips the trailing channel segment', () => {
  assert.equal(deviceOf('hwmon/nct6798/temp1'), 'hwmon/nct6798');
  assert.equal(
    deviceOf('corsair/commander-pro-0805009c9327/temp1'),
    'corsair/commander-pro-0805009c9327',
  );
  assert.equal(deviceOf('nvidia/0/temp'), 'nvidia/0');
});

test('sensorNodeId, controlNodeId, virtualNodeId, curveNodeId, combineNodeId prefix by kind', () => {
  assert.equal(sensorNodeId('hwmon/nct6798'), 'sensor:hwmon/nct6798');
  assert.equal(controlNodeId('hwmon/nct6798'), 'control:hwmon/nct6798');
  assert.equal(virtualNodeId('cpu_hot'), 'virtual:cpu_hot');
  assert.equal(curveNodeId('cpu'), 'curve:cpu');
  assert.equal(combineNodeId('m'), 'combine:m');
});

test('COMBINE_TYPES and isCombineType', () => {
  assert.deepEqual(COMBINE_TYPES, ['mix', 'sync']);
  assert.equal(isCombineType('mix'), true);
  assert.equal(isCombineType('sync'), true);
  assert.equal(isCombineType('point'), false);
});

test('nodeIdForCurveRef resolves combine vs curve vs missing', () => {
  const curvesById = {
    m: { type: 'mix', sources: [] },
    c: { type: 'point', sensor: '' },
  };
  assert.equal(nodeIdForCurveRef('m', curvesById), 'combine:m');
  assert.equal(nodeIdForCurveRef('c', curvesById), 'curve:c');
  assert.equal(nodeIdForCurveRef('ghost', curvesById), 'curve:ghost');
});

test('nodeKind and nodeName split on the first colon', () => {
  assert.equal(nodeKind('sensor:hwmon/nct6798'), 'sensor');
  assert.equal(nodeName('sensor:hwmon/nct6798'), 'hwmon/nct6798');
  assert.equal(nodeKind('virtual:cpu_hot'), 'virtual');
  assert.equal(nodeName('virtual:cpu_hot'), 'cpu_hot');
});

test('edgeId formats source and target with handles', () => {
  assert.equal(
    edgeId('sensor:hwmon/nct6798', 'hwmon/nct6798/temp1', 'virtual:cpu_hot', 'in-0'),
    'sensor:hwmon/nct6798:hwmon/nct6798/temp1->virtual:cpu_hot:in-0',
  );
});

test('virtualInputHandles is a single "in" for offset and delta regardless of count', () => {
  assert.deepEqual(virtualInputHandles('offset', 1), ['in']);
  assert.deepEqual(virtualInputHandles('delta', 1), ['in']);
});

test('virtualInputHandles is always numbered for max/min/mean, even with a single input', () => {
  assert.deepEqual(virtualInputHandles('max', 1), ['in-0']);
  assert.deepEqual(virtualInputHandles('min', 1), ['in-0']);
  assert.deepEqual(virtualInputHandles('mean', 1), ['in-0']);
  assert.deepEqual(virtualInputHandles('max', 3), ['in-0', 'in-1', 'in-2']);
});
