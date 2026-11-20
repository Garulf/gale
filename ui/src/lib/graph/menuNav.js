const STEPS = { ArrowDown: 1, ArrowUp: -1 };

export function nextMenuIndex(enabled, current, key) {
  const usable = enabled.map((on, index) => (on ? index : -1)).filter((index) => index >= 0);
  if (usable.length === 0) return -1;
  if (key === 'Home') return usable[0];
  if (key === 'End') return usable[usable.length - 1];
  const step = STEPS[key];
  if (!step) return current;
  const position = usable.indexOf(current);
  if (position < 0) return step > 0 ? usable[0] : usable[usable.length - 1];
  return usable[(position + step + usable.length) % usable.length];
}
