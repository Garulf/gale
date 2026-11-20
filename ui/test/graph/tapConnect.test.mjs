import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  connectionBetween,
  portTapState,
  tapPort,
  tapInstruction,
  nearestPort,
  portOfHandle,
  samePort,
  TAP_TARGET_PX,
} from '../../src/lib/graph/tapConnect.js';
import { isValidConnection } from '../../src/lib/graph/validate.js';

const sensorOut = { nodeId: 'sensor:it8792', handleId: 'hwmon/it8792/temp1', type: 'source', kind: 'temp' };
const curveIn = { nodeId: 'curve:cpu', handleId: 'sensor', type: 'target', kind: 'temp' };
const curveOut = { nodeId: 'curve:cpu', handleId: 'out', type: 'source', kind: 'duty' };
const fanIn = { nodeId: 'control:it8792', handleId: 'hwmon/it8792/pwm1', type: 'target', kind: 'duty' };
const rpmOut = { nodeId: 'sensor:fans', handleId: 'hwmon/it8792/fan1', type: 'source', kind: 'temp' };

const nodes = [
  { id: 'sensor:it8792', data: { deviceSensor: { rows: [{ handle: 'hwmon/it8792/temp1', kind: 'temp' }] } } },
  { id: 'sensor:fans', data: { deviceSensor: { rows: [{ handle: 'hwmon/it8792/fan1', kind: 'rpm' }] } } },
];
const valid = (connection) => isValidConnection(connection, nodes, []);

test('a source armed against a target makes a source to target connection', () => {
  assert.deepEqual(connectionBetween(sensorOut, curveIn), {
    source: 'sensor:it8792',
    sourceHandle: 'hwmon/it8792/temp1',
    target: 'curve:cpu',
    targetHandle: 'sensor',
  });
});

test('a target armed first still orients the connection source to target', () => {
  assert.deepEqual(connectionBetween(fanIn, curveOut), {
    source: 'curve:cpu',
    sourceHandle: 'out',
    target: 'control:it8792',
    targetHandle: 'hwmon/it8792/pwm1',
  });
});

test('two ports of the same direction never form a connection', () => {
  assert.equal(connectionBetween(sensorOut, curveOut), null);
  assert.equal(connectionBetween(null, curveOut), null);
});

test('nothing is marked while no port is armed', () => {
  assert.equal(portTapState(null, curveIn, valid), '');
});

test('compatible targets are offered and incompatible ones are blocked', () => {
  assert.equal(portTapState(sensorOut, sensorOut, valid), 'armed');
  assert.equal(portTapState(sensorOut, curveIn, valid), 'offered');
  assert.equal(portTapState(sensorOut, fanIn, valid), 'blocked');
  assert.equal(portTapState(sensorOut, curveOut, valid), 'blocked');
  assert.equal(portTapState(curveOut, fanIn, valid), 'offered');
});

test('a fan speed reading cannot drive a curve, so its curve input is not offered', () => {
  assert.equal(portTapState(rpmOut, curveIn, valid), 'blocked');
});

test('the first tap arms the port', () => {
  assert.deepEqual(tapPort(null, sensorOut, valid), { armed: sensorOut, connection: null });
});

test('tapping the armed port again disarms it', () => {
  assert.deepEqual(tapPort(sensorOut, { ...sensorOut }, valid), { armed: null, connection: null });
});

test('tapping another port of the same direction moves the arm to it', () => {
  assert.deepEqual(tapPort(sensorOut, curveOut, valid), { armed: curveOut, connection: null });
});

test('tapping a compatible target connects and disarms', () => {
  const result = tapPort(sensorOut, curveIn, valid);
  assert.equal(result.armed, null);
  assert.deepEqual(result.connection, connectionBetween(sensorOut, curveIn));
});

test('tapping an incompatible target keeps the arm and connects nothing', () => {
  assert.deepEqual(tapPort(sensorOut, fanIn, valid), { armed: sensorOut, connection: null });
});

test('the instruction names the kind and direction still needed', () => {
  assert.equal(tapInstruction(sensorOut), 'Tap a temperature input to connect');
  assert.equal(tapInstruction(curveOut), 'Tap a duty input to connect');
  assert.equal(tapInstruction(fanIn), 'Tap a duty source to connect');
  assert.equal(tapInstruction(curveIn), 'Tap a temperature source to connect');
  assert.equal(tapInstruction({ ...curveOut, kind: '' }), 'Tap an input to connect');
});

test('the nearest offered port within the tap target wins when hit areas overlap', () => {
  const candidates = [
    { port: 'a', x: 100, y: 100 },
    { port: 'b', x: 100, y: 116 },
  ];
  assert.equal(nearestPort({ x: 101, y: 110 }, candidates), 'b');
  assert.equal(nearestPort({ x: 101, y: 106 }, candidates), 'a');
});

test('a tap outside every tap target finds no port', () => {
  assert.equal(nearestPort({ x: 100 + TAP_TARGET_PX / 2 + 1, y: 100 }, [{ port: 'a', x: 100, y: 100 }]), null);
  assert.equal(nearestPort({ x: 122, y: 122 }, [{ port: 'a', x: 100, y: 100 }]), 'a');
  assert.ok(TAP_TARGET_PX >= 44);
});

function handleElement(nodeId, handleId, classes) {
  const attrs = { 'data-nodeid': nodeId, 'data-handleid': handleId };
  return { getAttribute: (name) => attrs[name], classList: { contains: (name) => classes.includes(name) } };
}

test('a handle element describes its node, handle, direction and kind', () => {
  const element = handleElement('curve:cpu', 'out', ['svelte-flow__handle', 'source', 'port', 'duty']);
  assert.ok(samePort(portOfHandle(element), curveOut));
  assert.equal(portOfHandle(element).kind, 'duty');
});

test('a plain port carries temperature and a kindless hollow port carries no kind', () => {
  assert.equal(portOfHandle(handleElement('sensor:it8792', 'hwmon/it8792/temp1', ['source', 'port'])).kind, 'temp');
  assert.equal(portOfHandle(handleElement('port:in:g', 'new', ['source', 'port', 'hollow'])).kind, '');
  assert.equal(portOfHandle(handleElement('port:in:g', 'a|b', ['source', 'port', 'duty', 'hollow'])).kind, 'duty');
  assert.equal(portOfHandle(handleElement('curve:cpu', 'sensor', ['target', 'port'])).type, 'target');
});
