export function sparkPath(points, width, height) {
  if (!points || points.length === 0) return '';
  const sorted = [...points].sort((a, b) => a[0] - b[0]);
  return sorted
    .map(([temp, duty], index) => {
      const x = (temp / 100) * width;
      const y = height - (duty / 100) * height;
      return `${index === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`;
    })
    .join(' ');
}
