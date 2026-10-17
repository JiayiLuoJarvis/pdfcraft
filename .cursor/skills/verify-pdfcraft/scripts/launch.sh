#!/usr/bin/env bash
# Build example binaries once so later drives do not recompile cold.
set -euo pipefail
# shellcheck source=env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/env.sh"
export_fonts

log="${EVIDENCE_DIR}/launch.log"
{
  echo "repo=${REPO_ROOT}"
  echo "run_id=${RUN_ID}"
  echo "PDFCRAFT_PDF_FONT=${PDFCRAFT_PDF_FONT}"
  echo "PDFCRAFT_FORMS_FONT=${PDFCRAFT_FORMS_FONT}"
  echo "PDFCRAFT_ORGCHART_FONT=${PDFCRAFT_ORGCHART_FONT}"
} | tee "${log}"

cargo build -p pdfcraft-pdf --example hello --example bill \
  -p pdfcraft-forms --example fill_basic \
  -p pdfcraft-orgchart --example equity_chart \
  -p pdfcraft-post --example encrypt --example merge --example stamp \
  2>&1 | tee -a "${log}"

# Ready = examples exist under target/debug/examples/
for name in hello fill_basic equity_chart encrypt merge; do
  bin="${REPO_ROOT}/target/debug/examples/${name}"
  if [[ ! -x "${bin}" ]]; then
    echo "launch: missing binary ${bin}" >&2
    exit 1
  fi
  echo "ready binary ${name}" | tee -a "${log}"
done

echo "launch: ready (examples built)" | tee -a "${log}"
echo "${RUN_ID}" > "${RUN_DIR}/.launched"
echo "VERIFY_RUN_ID=${RUN_ID}"
echo "VERIFY_RUN_DIR=${RUN_DIR}"
