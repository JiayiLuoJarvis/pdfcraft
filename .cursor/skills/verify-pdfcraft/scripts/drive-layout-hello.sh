#!/usr/bin/env bash
# User path: cargo example hello → PDF with layout tour content.
set -euo pipefail
# shellcheck source=env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/env.sh"
export_fonts

if [[ ! -f "${RUN_DIR}/.launched" ]]; then
  echo "drive: run scripts/launch.sh first (missing ${RUN_DIR}/.launched)" >&2
  exit 1
fi

out_pdf="${SCRATCH_DIR}/hello.pdf"
action_log="${EVIDENCE_DIR}/layout-hello.action.log"
proof_dir="${EVIDENCE_DIR}/layout-hello"
mkdir -p "${proof_dir}"

{
  echo "feature=layout-hello"
  echo "cmd=cargo run -p pdfcraft-pdf --example hello -- ${out_pdf}"
  echo "font=${PDFCRAFT_PDF_FONT}"
} | tee "${action_log}"

cargo run -p pdfcraft-pdf --example hello -- "${out_pdf}" 2>&1 | tee -a "${action_log}"

cp -f "${out_pdf}" "${proof_dir}/hello.pdf"

{
  echo "=== pdfinfo ==="
  pdfinfo "${proof_dir}/hello.pdf"
  echo "=== head bytes / magic ==="
  head -c 8 "${proof_dir}/hello.pdf" | od -An -tx1
  echo "=== pdftotext (first 40 lines) ==="
  pdftotext -layout "${proof_dir}/hello.pdf" - | head -n 40
} | tee "${proof_dir}/observe.txt"

# Observable end state: valid PDF magic, multi-page, tour title present.
if ! head -c 5 "${proof_dir}/hello.pdf" | grep -q '%PDF-'; then
  echo "drive: missing %PDF- magic" >&2
  exit 1
fi
pages="$(pdfinfo "${proof_dir}/hello.pdf" | awk -F: '/^Pages:/ {gsub(/ /,"",$2); print $2}')"
if [[ "${pages}" -lt 2 ]]; then
  echo "drive: expected >=2 pages, got ${pages}" >&2
  exit 1
fi
if ! pdftotext -layout "${proof_dir}/hello.pdf" - | grep -q 'Quire layout tour'; then
  echo "drive: missing expected title text" >&2
  exit 1
fi

echo "drive layout-hello: ok → ${proof_dir}"
