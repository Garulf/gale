export function shouldSnapshotEdit(lastEditAt, now, debounceMs) {
  return lastEditAt === null || now - lastEditAt >= debounceMs;
}
