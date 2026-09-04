export const INITIAL_VISIBLE_ROWS = 3;

export function foldRows(rows, initialVisible = INITIAL_VISIBLE_ROWS) {
  const wiredCount = rows.filter((row) => row.wired).length;
  let unwiredSlots = Math.max(0, initialVisible - wiredCount);
  const visible = [];
  const hidden = [];
  for (const row of rows) {
    if (row.wired) {
      visible.push(row);
    } else if (unwiredSlots > 0) {
      unwiredSlots -= 1;
      visible.push(row);
    } else {
      hidden.push(row);
    }
  }
  return { visible, hidden };
}
