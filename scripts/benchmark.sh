#!/bin/sh
set -eu

[ "$#" -ge 1 ] || { echo "Usage: $0 INPUT_DIRECTORY [OUTPUT]" >&2; exit 2; }
INPUT=$(cd "$1" 2>/dev/null && pwd) || { echo "Input must be a directory." >&2; exit 1; }
OUTPUT=${2:-"$(dirname "$INPUT")/$(basename "$INPUT").benchmark.tar.zst"}
[ ! -e "$OUTPUT" ] || { echo "Output already exists: $OUTPUT" >&2; exit 1; }
command -v fcz >/dev/null 2>&1 || { echo "fcz is not in PATH." >&2; exit 1; }

FILES=$(find "$INPUT" -type f -print | wc -l | tr -d ' ')
FOLDERS=$(find "$INPUT" -mindepth 1 -type d -print | wc -l | tr -d ' ')
BYTES=$(find "$INPUT" -type f -exec wc -c {} + | awk '$2 != "total" { sum += $1 } END { print sum + 0 }')

echo "fcz benchmark"
echo "System: $(uname -srm)"
echo "Logical processors: $(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.logicalcpu 2>/dev/null || echo unknown)"
echo "Input: $INPUT"
echo "Files: $FILES; folders: $FOLDERS; bytes: $BYTES"

START=$(date +%s)
fcz compress "$INPUT" --output "$OUTPUT"
END=$(date +%s)
SECONDS=$((END - START))
[ "$SECONDS" -gt 0 ] || SECONDS=1
if stat -c %s "$OUTPUT" >/dev/null 2>&1; then
  OUTPUT_BYTES=$(stat -c %s "$OUTPUT")
else
  OUTPUT_BYTES=$(stat -f %z "$OUTPUT")
fi
awk -v bytes="$BYTES" -v out="$OUTPUT_BYTES" -v seconds="$SECONDS" 'BEGIN {
  printf "Elapsed: %d seconds\n", seconds
  printf "Archive bytes: %d\n", out
  printf "Approximate throughput: %.2f MiB/s\n", bytes / seconds / 1048576
  printf "Compression ratio: %.2f%% (archive/input)\n", (bytes > 0 ? 100 * out / bytes : 0)
}'
echo "Output: $OUTPUT"
