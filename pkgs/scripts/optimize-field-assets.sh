#!/usr/bin/env bash
# Rebuild the production FGC field visual with glTF Transform.
#
# Default: atomically replaces pkgs/games/fgc-2026/field.glb.
# Example candidate build:
#   pkgs/scripts/optimize-field-assets.sh --output /tmp/field.optimized.glb
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
SOURCE="$PACKAGE_ROOT/games/fgc-2026/field.glb"
OUTPUT="$SOURCE"
# Retain enough detail for the playfield while eliminating CAD-level tessellation
# that does not contribute at match-camera distances. Callers may override these
# for asset-review builds, e.g. SIMPLIFY_RATIO=0.6 script --output /tmp/field.glb.
SIMPLIFY_RATIO="${SIMPLIFY_RATIO:-0.35}"
SIMPLIFY_ERROR="${SIMPLIFY_ERROR:-0.001}"

if [[ "${1:-}" == "--output" ]]; then
  [[ -n "${2:-}" ]] || { echo "--output needs a destination path" >&2; exit 2; }
  OUTPUT="$2"
  shift 2
fi
[[ $# -eq 0 ]] || { echo "Usage: $0 [--output path]" >&2; exit 2; }
[[ -f "$SOURCE" ]] || { echo "Missing source GLB: $SOURCE" >&2; exit 1; }

# glTF Transform selects its container from the extension. Keep `.glb` as the
# final suffix so the temporary build is a self-contained binary GLB rather
# than a JSON glTF with a sidecar `.bin` file.
OUTPUT_DIR="$(dirname -- "$OUTPUT")"
OUTPUT_NAME="$(basename -- "$OUTPUT")"
case "$OUTPUT_NAME" in
  *.glb) TMP_OUTPUT="$OUTPUT_DIR/.${OUTPUT_NAME%.glb}.tmp.$$.glb" ;;
  *) echo "Output must have a .glb extension: $OUTPUT" >&2; exit 2 ;;
esac
cleanup() { rm -f -- "$TMP_OUTPUT"; }
trap cleanup EXIT

echo "Optimizing $(basename "$SOURCE")…"
echo "  input:  $(du -h "$SOURCE" | cut -f1)"

# glTF Transform's optimize pipeline removes unused data, deduplicates meshes,
# instances repeated mesh data, applies Meshopt geometry compression, and
# transcodes textures to WebP. Palette conversion is intentionally disabled:
# the runtime needs the source's dedicated clear-polycarbonate material to
# remain separate from opaque goal backing panels. The constrained
# simplification preserves mesh borders, avoiding visible gaps in the field
# while reducing render workload.
# Using pnpm dlx keeps the game package self-contained and reproducible by CLI
# major.
pnpm --silent dlx @gltf-transform/cli@4 optimize \
  "$SOURCE" "$TMP_OUTPUT" \
  --palette false \
  --texture-compress webp \
  --simplify-ratio "$SIMPLIFY_RATIO" \
  --simplify-error "$SIMPLIFY_ERROR" \
  --simplify-lock-border true

[[ -s "$TMP_OUTPUT" ]] || { echo "Optimizer did not produce an output GLB" >&2; exit 1; }
mkdir -p -- "$(dirname -- "$OUTPUT")"
mv -- "$TMP_OUTPUT" "$OUTPUT"
trap - EXIT

echo "  output: $(du -h "$OUTPUT" | cut -f1)"
echo "Done. The game-pack manifest continues to reference field.glb."
