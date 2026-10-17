#!/usr/bin/env bash
# User path: equity_chart example → orgchart PDF.
# Note: example always writes workspace out/equity_chart.pdf (not isolatable).
set -euo pipefail
# shellcheck source=env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/env.sh"
export_fonts

if [[ ! -f "${RUN_DIR}/.launched" ]]; then
  echo "drive: run scripts/launch.sh first" >&2
  exit 1
fi

lock="${REPO_ROOT}/out/.verify-equity.lock"
mkdir -p "${REPO_ROOT}/out"
proof_dir="${EVIDENCE_DIR}/orgchart-equity"
mkdir -p "${proof_dir}"
action_log="${EVIDENCE_DIR}/orgchart-equity.action.log"
shared_out="${REPO_ROOT}/out/equity_chart.pdf"

exec 9>"${lock}"
if ! flock -n 9; then
  echo "drive: another equity_chart run holds ${lock}; refuse concurrent drive" >&2
  exit 1
fi

{
  echo "feature=orgchart-equity"
  echo "cmd=cargo run -p pdfcraft-orgchart --example equity_chart"
  echo "font=${PDFCRAFT_ORGCHART_FONT}"
  echo "shared_out=${shared_out}"
} | tee "${action_log}"

rm -f "${shared_out}"
cargo run -p pdfcraft-orgchart --example equity_chart 2>&1 | tee -a "${action_log}"

cp -f "${shared_out}" "${proof_dir}/equity_chart.pdf"
# Release shared path so other work is not left holding our bytes.
rm -f "${shared_out}"

{
  echo "=== pdfinfo ==="
  pdfinfo "${proof_dir}/equity_chart.pdf"
  echo "=== pdftotext ==="
  pdftotext -layout "${proof_dir}/equity_chart.pdf" - || true
} | tee "${proof_dir}/observe.txt"

if ! head -c 5 "${proof_dir}/equity_chart.pdf" | grep -q '%PDF-'; then
  echo "drive: missing %PDF- magic" >&2
  exit 1
fi
if ! pdftotext -layout "${proof_dir}/equity_chart.pdf" - | grep -Eq '主体公司|甲股东|乙投资'; then
  echo "drive: expected chart labels missing from pdftotext" >&2
  exit 1
fi

echo "drive orgchart-equity: ok → ${proof_dir}"
