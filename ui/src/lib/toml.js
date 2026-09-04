const BARE_KEY = /^[A-Za-z0-9_-]+$/;

function key(name) {
  return BARE_KEY.test(name) ? name : JSON.stringify(name);
}

function isTable(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function scalar(value) {
  if (typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  if (Array.isArray(value)) return `[${value.map(scalar).join(', ')}]`;
  if (isTable(value)) {
    const entries = Object.entries(value).map(([k, v]) => `${key(k)} = ${scalar(v)}`);
    return `{ ${entries.join(', ')} }`;
  }
  return JSON.stringify(value);
}

function isArrayOfTables(value) {
  return Array.isArray(value) && value.length > 0 && value.every(isTable);
}

function render(table, path, out) {
  const scalars = [];
  const subtables = [];
  for (const [name, value] of Object.entries(table)) {
    if (value === null || value === undefined) continue;
    if (isTable(value)) subtables.push([name, value]);
    else if (isArrayOfTables(value)) subtables.push([name, value]);
    else scalars.push([name, value]);
  }
  if (path.length > 0 && (scalars.length > 0 || subtables.length === 0)) {
    if (out.length > 0) out.push('');
    out.push(`[${path.map(key).join('.')}]`);
  }
  for (const [name, value] of scalars) out.push(`${key(name)} = ${scalar(value)}`);
  for (const [name, value] of subtables) {
    if (Array.isArray(value)) {
      for (const item of value) {
        out.push('');
        out.push(`[[${[...path, name].map(key).join('.')}]]`);
        render(item, [], out);
      }
    } else {
      render(value, [...path, name], out);
    }
  }
}

export function toToml(value) {
  const out = [];
  render(value || {}, [], out);
  return out.join('\n');
}
