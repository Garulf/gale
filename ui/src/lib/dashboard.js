export function shortDevice(device) {
  if (device.startsWith('hwmon/')) return device.slice('hwmon/'.length);
  if (device.startsWith('corsair/commander-pro-')) return `cmd-pro ${device.slice('corsair/commander-pro-'.length)}`;
  if (device.startsWith('corsair/')) return device.slice('corsair/'.length);
  if (device.startsWith('nvidia/')) return `nvidia gpu ${device.slice('nvidia/'.length)}`;
  return device;
}

export function deviceOf(id) {
  const slash = id.lastIndexOf('/');
  return slash === -1 ? id : id.slice(0, slash);
}

export function headline({ maxTemp, avgDuty, warnings }) {
  if (warnings > 0) return warnings === 1 ? 'One warning needs attention' : `${warnings} warnings need attention`;
  if (maxTemp !== null && maxTemp >= 80) return 'System is running hot';
  if (avgDuty !== null && avgDuty >= 70) return 'Fans are working hard';
  return 'System is cool and quiet';
}

export function overview(temps, duties, warningCount) {
  const tempValues = temps.filter((value) => value !== null && value !== undefined);
  const dutyValues = duties.filter((value) => value !== null && value !== undefined);
  const maxTemp = tempValues.length ? Math.max(...tempValues) : null;
  const avgDuty = dutyValues.length ? dutyValues.reduce((sum, value) => sum + value, 0) / dutyValues.length : null;
  return { maxTemp, avgDuty, warnings: warningCount, headline: headline({ maxTemp, avgDuty, warnings: warningCount }) };
}

export function trendArrow(delta) {
  if (delta > 0.5) return '↗';
  if (delta < -0.5) return '↘';
  return '→';
}
