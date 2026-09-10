#!/usr/bin/env bash
# Rebuild the production robot visual with glTF Transform.
#
# Preserves node hierarchy, scene roots, and animations/mechanisms
# (e.g. IntakeRoller, OuttakeRoller, TransferRoller).
#
# Default: atomically replaces pkgs/games/fgc-2026/robots/starter-bot/bot.glb.
# Example candidate build:
#   pkgs/scripts/optimize-robot-assets.sh --output /tmp/bot.optimized.glb
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
SOURCE="$PACKAGE_ROOT/games/fgc-2026/robots/starter-bot/bot.glb"
OUTPUT="$SOURCE"

SIMPLIFY_RATIO="${SIMPLIFY_RATIO:-0.35}"
SIMPLIFY_ERROR="${SIMPLIFY_ERROR:-0.001}"

if [[ "${1:-}" == "--output" ]]; then
  [[ -n "${2:-}" ]] || { echo "--output needs a destination path" >&2; exit 2; }
  OUTPUT="$2"
  shift 2
fi
[[ $# -eq 0 ]] || { echo "Usage: $0 [--output path]" >&2; exit 2; }
[[ -f "$SOURCE" ]] || { echo "Missing source GLB: $SOURCE" >&2; exit 1; }

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

# Note: --flatten false, --join false, and --instance false are critical
# to preserve the exact node tree and hierarchy for moving parts
# (such as IntakeRoller, OuttakeRoller, TransferRoller, and Wheels).
pnpm --silent dlx @gltf-transform/cli@4 optimize \
  "$SOURCE" "$TMP_OUTPUT" \
  --flatten false \
  --join false \
  --instance false \
  --palette false \
  --simplify-ratio "$SIMPLIFY_RATIO" \
  --simplify-error "$SIMPLIFY_ERROR" \
  --simplify-lock-border true \
  --compress meshopt \
  --meshopt-level high

[[ -s "$TMP_OUTPUT" ]] || { echo "Optimizer did not produce an output GLB" >&2; exit 1; }
mkdir -p -- "$(dirname -- "$OUTPUT")"
mv -- "$TMP_OUTPUT" "$OUTPUT"
trap - EXIT

echo "  output: $(du -h "$OUTPUT" | cut -f1)"
echo "Done. The robot visual GLB structure is preserved."
