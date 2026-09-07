import { isCurveInputKind } from './liveValues.js';

export const INITIAL_VISIBLE_ROWS = 3;

function isCurveInput(row) {
  return isCurveInputKind(row.kind);
}

export function foldRows(rows, initialVisible = INITIAL_VISIBLE_ROWS) {
  const wiredCount = rows.filter((row) => row.wired).length;
  const unwiredSlots = Math.max(0, initialVisible - wiredCount);

  const unwiredRows = rows.filter((row) => !row.wired);
  const prioritized = [
    ...unwiredRows.filter(isCurveInput),
    ...unwiredRows.filter((row) => !isCurveInput(row)),
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
