#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
assets_dir="${repo_root}/assets"
src_svg="${assets_dir}/gale.svg"

report() {
    local path="$1"
    local size
    size=$(stat -c '%s' "${path}")
    echo "${path} (${size} bytes)"
}

render_recolored_rgba() {
    local color="$1"
    local out_path="$2"
    local tmp_svg
    tmp_svg=$(mktemp --suffix=.svg)
    sed "s/#1f7fd6/${color}/g" "${src_svg}" > "${tmp_svg}"
    rsvg-convert -w 32 -h 32 --format png "${tmp_svg}" | python3 -c "
import sys
from PIL import Image
img = Image.open(sys.stdin.buffer).convert('RGBA')
with open('${out_path}', 'wb') as f:
    f.write(img.tobytes())
"
    rm -f "${tmp_svg}"
}

mkdir -p "${assets_dir}"

ico_sizes=(16 24 32 48 256)
ico_png_dir=$(mktemp -d)
trap 'rm -rf "${ico_png_dir}"' EXIT

png_args=()
for size in "${ico_sizes[@]}"; do
    png_path="${ico_png_dir}/${size}.png"
    rsvg-convert -w "${size}" -h "${size}" --format png "${src_svg}" -o "${png_path}"
    png_args+=("${png_path}")
done

python3 - "${assets_dir}/gale.ico" "${png_args[@]}" <<'PYEOF'
import struct
import sys
from io import BytesIO

from PIL import Image

out_path = sys.argv[1]
png_paths = sys.argv[2:]

entries = []
for path in png_paths:
    image = Image.open(path).convert("RGBA")
    width, height = image.size
    buffer = BytesIO()
    image.save(buffer, format="PNG")
    entries.append((width, height, buffer.getvalue()))

header = struct.pack("<HHH", 0, 1, len(entries))
directory = b""
image_data = b""
offset = 6 + 16 * len(entries)
for width, height, data in entries:
    directory += struct.pack(
        "<BBBBHHII",
        width if width < 256 else 0,
        height if height < 256 else 0,
        0,
        0,
        1,
        32,
        len(data),
        offset,
    )
    image_data += data
    offset += len(data)

with open(out_path, "wb") as f:
    f.write(header)
    f.write(directory)
    f.write(image_data)
PYEOF

report "${assets_dir}/gale.ico"

render_recolored_rgba "#ffffff" "${assets_dir}/tray-32.rgba"
report "${assets_dir}/tray-32.rgba"

render_recolored_rgba "#1a2330" "${assets_dir}/tray-32-dark.rgba"
report "${assets_dir}/tray-32-dark.rgba"

cp "${src_svg}" "${repo_root}/ui/public/favicon.svg"
report "${repo_root}/ui/public/favicon.svg"
