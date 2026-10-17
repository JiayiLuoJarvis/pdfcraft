#!/usr/bin/env bash
# User path: fill_basic example → flattened form PDF with filled values.
set -euo pipefail
# shellcheck source=env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/env.sh"
export_fonts

if [[ ! -f "${RUN_DIR}/.launched" ]]; then
  echo "drive: run scripts/launch.sh first" >&2
  exit 1
fi

# fill_basic writes cwd/out/fill_basic.pdf — isolate via scratch cwd.
work="${SCRATCH_DIR}/forms-fill"
mkdir -p "${work}/out"
proof_dir="${EVIDENCE_DIR}/forms-fill"
mkdir -p "${proof_dir}"
action_log="${EVIDENCE_DIR}/forms-fill.action.log"

{
  echo "feature=forms-fill"
  echo "cmd=cargo run -p pdfcraft-forms --example fill_basic  (cwd=${work})"
  echo "font=${PDFCRAFT_FORMS_FONT}"
} | tee "${action_log}"

(
  cd "${work}"
  cargo run -p pdfcraft-forms --example fill_basic --manifest-path "${REPO_ROOT}/Cargo.toml"
) 2>&1 | tee -a "${action_log}"

cp -f "${work}/out/fill_basic.pdf" "${proof_dir}/fill_basic.pdf"

{
  echo "=== pdfinfo ==="
  pdfinfo "${proof_dir}/fill_basic.pdf"
  echo "=== pdftotext ==="
  pdftotext -layout "${proof_dir}/fill_basic.pdf" - || true
} | tee "${proof_dir}/observe.txt"

if ! head -c 5 "${proof_dir}/fill_basic.pdf" | grep -q '%PDF-'; then
  echo "drive: missing %PDF- magic" >&2
  exit 1
fi
if ! pdftotext -layout "${proof_dir}/fill_basic.pdf" - | grep -q 'Ada'; then
  echo "drive: filled text 'Ada' not found via pdftotext" >&2
  exit 1
fi

echo "drive forms-fill: ok → ${proof_dir}"
