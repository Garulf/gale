const CHANNEL = /(\d+)$/;

function channelOf(id) {
  const leaf = id.slice(id.lastIndexOf('/') + 1);
  const match = CHANNEL.exec(leaf);
  return match ? match[1] : null;
}

function deviceOf(id) {
  return id.slice(0, id.lastIndexOf('/'));
}

export function tachSensorFor(controlId, sensors) {
  const rpmSensors = (sensors || []).filter((sensor) => sensor.kind === 'rpm');
  const sameDevice = rpmSensors.filter((sensor) => deviceOf(sensor.id) === deviceOf(controlId));
  if (sameDevice.length === 0) return null;
  const exact = sameDevice.find((sensor) => sensor.id === controlId);
  if (exact) return exact;
  const channel = channelOf(controlId);
  if (channel !== null) {
    const byChannel = sameDevice.find((sensor) => channelOf(sensor.id) === channel);
    if (byChannel) return byChannel;
  }
  return sameDevice.length === 1 ? sameDevice[0] : null;
}
