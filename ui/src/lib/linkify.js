export function splitLinks(text) {
  const parts = [];
  let cursor = 0;
  const pattern = /https:\/\/\S+/g;
  let match;
  while ((match = pattern.exec(text)) !== null) {
    if (match.index > cursor) {
      parts.push({ text: text.slice(cursor, match.index) });
    }
    let href = match[0];
    let trailing = '';
    if (href.endsWith('.') || href.endsWith(',')) {
      trailing = href.slice(-1);
      href = href.slice(0, -1);
    }
    parts.push({ href });
    cursor = match.index + match[0].length;
    if (trailing) {
      parts.push({ text: trailing });
    }
  }
  if (cursor < text.length) {
    const rest = text.slice(cursor);
    const last = parts[parts.length - 1];
    if (last && 'text' in last) {
      last.text += rest;
    } else {
      parts.push({ text: rest });
    }
  }
  if (parts.length === 0) {
    parts.push({ text });
  }
  return parts;
}
