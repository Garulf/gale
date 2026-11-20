export const TAP_TARGET_PX = 48;

export function samePort(a, b) {
  return Boolean(a && b) && a.nodeId === b.nodeId && a.handleId === b.handleId && a.type === b.type;
}

export function connectionBetween(armed, port) {
  if (!armed || !port || armed.type === port.type) return null;
  const [from, to] = armed.type === 'source' ? [armed, port] : [port, armed];
  return { source: from.nodeId, sourceHandle: from.handleId, target: to.nodeId, targetHandle: to.handleId };
}

export function portTapState(armed, port, isValid) {
  if (!armed) return '';
  if (samePort(armed, port)) return 'armed';
  const connection = connectionBetween(armed, port);
  return connection && isValid(connection) ? 'offered' : 'blocked';
}

export function tapPort(armed, port, isValid) {
  if (!armed) return { armed: port, connection: null };
  if (samePort(armed, port)) return { armed: null, connection: null };
  const connection = connectionBetween(armed, port);
  if (!connection) return { armed: port, connection: null };
  return isValid(connection) ? { armed: null, connection } : { armed, connection: null };
}

const KIND_WORDS = { temp: 'a temperature', duty: 'a duty' };

export function tapInstruction(armed) {
  const kind = KIND_WORDS[armed.kind] || 'an';
  return armed.type === 'source' ? `Tap ${kind} input to connect` : `Tap ${kind} source to connect`;
}

export function nearestPort(point, candidates, size = TAP_TARGET_PX) {
  const reach = size / 2;
  let best = null;
  let bestDistance = Infinity;
  for (const candidate of candidates) {
    const dx = Math.abs(point.x - candidate.x);
    const dy = Math.abs(point.y - candidate.y);
    if (dx > reach || dy > reach) continue;
    const distance = dx * dx + dy * dy;
    if (distance < bestDistance) {
      best = candidate.port;
      bestDistance = distance;
    }
  }
  return best;
}

function handleKind(classList) {
  if (classList.contains('duty')) return 'duty';
  if (classList.contains('temp')) return 'temp';
  return classList.contains('hollow') ? '' : 'temp';
}

export function portOfHandle(element) {
  return {
    nodeId: element.getAttribute('data-nodeid'),
    handleId: element.getAttribute('data-handleid'),
    type: element.classList.contains('target') ? 'target' : 'source',
    kind: handleKind(element.classList),
  };
}
