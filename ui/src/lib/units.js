export const KIND_UNITS = {
  temp: '°C',
  rpm: 'rpm',
  duty: '%',
  percent: '%',
  clock: 'MHz',
  memory: 'MiB',
  power: 'W',
  state: '',
};

export function axisMark(unit) {
  if (!unit) return '';
  return unit === KIND_UNITS.temp ? '°' : unit;
}
