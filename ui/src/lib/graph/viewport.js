export const MIN_ZOOM = 0.15;
export const MAX_FIT_ZOOM = 1.1;
export const NARROW_QUERY = '(max-width: 720px)';
export const COARSE_POINTER_QUERY = '(pointer: coarse)';
export const OVERVIEW_ZOOM = 0.6;

export function showsEdgeLabel(zoom, emphasized) {
  return emphasized || zoom >= OVERVIEW_ZOOM;
}

const NARROW_INSET = { top: 16, right: 16, bottom: 80, left: 16 };
const WIDE_INSET = { top: 72, right: 32, bottom: 64, left: 32 };

function toPixels(inset) {
  return Object.fromEntries(Object.entries(inset).map(([side, value]) => [side, `${value}px`]));
}

export function fitOptions(narrow) {
  return { padding: toPixels(narrow ? NARROW_INSET : WIDE_INSET), maxZoom: MAX_FIT_ZOOM, minZoom: MIN_ZOOM };
}

export function canvasInset(narrow) {
  return narrow ? NARROW_INSET : WIDE_INSET;
}

export function isNarrowViewport() {
  return typeof window !== 'undefined' && window.matchMedia(NARROW_QUERY).matches;
}

export function canFit(store) {
  return store.width > 0 && store.height > 0 && store.nodes.length > 0 && store.nodesInitialized;
}

function shiftInto(start, length, min, available) {
  if (start < min) return min - start;
  const overflow = start + length - (min + available);
  return overflow > 0 ? -overflow : 0;
}

export function revealViewport(bounds, viewport, size, inset = WIDE_INSET) {
  const availableWidth = size.width - inset.left - inset.right;
  const availableHeight = size.height - inset.top - inset.bottom;
  if (availableWidth <= 0 || availableHeight <= 0 || !(bounds.width > 0) || !(bounds.height > 0)) return null;
  const { zoom } = viewport;
  const screenX = bounds.x * zoom + viewport.x;
  const screenY = bounds.y * zoom + viewport.y;
  if (bounds.width * zoom <= availableWidth && bounds.height * zoom <= availableHeight) {
    const dx = shiftInto(screenX, bounds.width * zoom, inset.left, availableWidth);
    const dy = shiftInto(screenY, bounds.height * zoom, inset.top, availableHeight);
    if (dx === 0 && dy === 0) return null;
    return { x: viewport.x + dx, y: viewport.y + dy, zoom };
  }
  const fitted = Math.max(MIN_ZOOM, Math.min(zoom, availableWidth / bounds.width, availableHeight / bounds.height));
  return {
    x: inset.left + (availableWidth - bounds.width * fitted) / 2 - bounds.x * fitted,
    y: inset.top + (availableHeight - bounds.height * fitted) / 2 - bounds.y * fitted,
    zoom: fitted,
  };
}
