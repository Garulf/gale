import { isCurveInputKind } from './liveValues.js';

export const INITIAL_VISIBLE_ROWS = 3;

function tier(row) {
  if (row.kind === undefined || row.kind === 'temp') return 0;
  return isCurveInputKind(row.kind) ? 1 : 2;
}

export function foldRows(rows, initialVisible = INITIAL_VISIBLE_ROWS) {
  const wiredCount = rows.filter((row) => row.wired).length;
  const unwiredSlots = Math.max(0, initialVisible - wiredCount);

  const unwiredRows = rows.filter((row) => !row.wired);
  const prioritized = [
    ...unwiredRows.filter((row) => tier(row) === 0),
    ...unwiredRows.filter((row) => tier(row) === 1),
    ...unwiredRows.filter((row) => tier(row) === 2),
  ];
  const selected = new Set(prioritized.slice(0, unwiredSlots));

  const visible = [];
  const hidden = [];
  for (const row of rows) {
    if (row.wired || selected.has(row)) {
      visible.push(row);
    } else {
      hidden.push(row);
    }
  }
  return { visible, hidden };
}

export function shownRows(rows) {
  return rows.filter((row) => !row.hidden || row.wired);
}
