export const PALETTE = [
  {
    label: 'Curves',
    entries: [
      { kind: 'curve', type: 'point', label: 'Point curve', hint: 'draw duty against temperature' },
      { kind: 'curve', type: 'linear', label: 'Linear curve', hint: 'two ends, straight ramp' },
      { kind: 'curve', type: 'trigger', label: 'Trigger', hint: 'on above, off below' },
      { kind: 'curve', type: 'target', label: 'Target', hint: 'hold a temperature' },
      { kind: 'curve', type: 'flat', label: 'Flat', hint: 'fixed duty' },
    ],
  },
  {
    label: 'Temperature',
    entries: [
      { kind: 'virtual', type: 'max', label: 'Max', hint: 'hottest of its inputs' },
      { kind: 'virtual', type: 'min', label: 'Min', hint: 'coolest of its inputs' },
      { kind: 'virtual', type: 'mean', label: 'Average', hint: 'mean of its inputs' },
      { kind: 'virtual', type: 'offset', label: 'Offset', hint: 'add and scale one input' },
      { kind: 'virtual', type: 'delta', label: 'Delta', hint: 'rate of change per minute' },
      { kind: 'virtual', type: 'webhook', label: 'Webhook', hint: 'value pushed over HTTP' },
    ],
  },
  {
    label: 'Duty',
    entries: [
      { kind: 'combine', type: 'mix', mode: 'max', label: 'Max', hint: 'highest of its inputs' },
      { kind: 'combine', type: 'mix', mode: 'min', label: 'Min', hint: 'lowest of its inputs' },
      { kind: 'combine', type: 'mix', mode: 'avg', label: 'Average', hint: 'mean of its inputs' },
      { kind: 'combine', type: 'offset', label: 'Offset', hint: 'add and scale one input' },
      { kind: 'combine', type: 'sync', label: 'Sync', hint: 'follow another curve' },
    ],
  },
];

export function paletteEntryId(entry) {
  return [entry.kind, entry.type, entry.mode].filter(Boolean).join('-');
}

const OPERATION_NAMES = { mean: 'average', avg: 'average' };

function operationName(word) {
  return OPERATION_NAMES[word] || word;
}

export function operationLabel(nodeType, config) {
  if (nodeType === 'virtual') return `temperature · ${operationName(config.type)}`;
  if (nodeType === 'combine') return `duty · ${operationName(config.type === 'mix' ? config.mode : config.type)}`;
  return `${config.type} curve`;
}
