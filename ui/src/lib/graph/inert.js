export function inertOutside(element) {
  const changed = [];
  let current = element;
  while (current && current.parentElement) {
    for (const sibling of current.parentElement.children) {
      if (sibling === current || sibling.inert) continue;
      sibling.inert = true;
      changed.push(sibling);
    }
    current = current.parentElement;
  }
  return () => {
    for (const sibling of changed) sibling.inert = false;
  };
}
