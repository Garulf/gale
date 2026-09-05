import { deviceOf } from './graph/ids.js';

const CHANNEL = /(\d+)$/;

function channelOf(id) {
  const leaf = id.slice(id.lastIndexOf('/') + 1);
  const match = CHANNEL.exec(leaf);
  return match ? match[1] : null;
}

export function tachSensorFor(controlId, sensors) {
  const sameDevice = (sensors || []).filter((sensor) => sensor.kind === 'rpm' && deviceOf(sensor.id) === deviceOf(controlId));
  const exact = sameDevice.find((sensor) => sensor.id === controlId);
  if (exact) return exact;
  const channel = channelOf(controlId);
  if (channel === null) return null;
  return sameDevice.find((sensor) => channelOf(sensor.id) === channel) || null;
}
