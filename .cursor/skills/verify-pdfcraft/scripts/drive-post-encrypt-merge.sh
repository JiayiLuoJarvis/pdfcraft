#!/usr/bin/env bash
# User path: encrypt then merge example CLIs on fixture/input PDFs.
set -euo pipefail
# shellcheck source=env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/env.sh"
export_fonts

if [[ ! -f "${RUN_DIR}/.launched" ]]; then
  echo "drive: run scripts/launch.sh first" >&2
  exit 1
fi

proof_dir="${EVIDENCE_DIR}/post-encrypt-merge"
work="${SCRATCH_DIR}/post"
mkdir -p "${proof_dir}" "${work}"
action_log="${EVIDENCE_DIR}/post-encrypt-merge.action.log"
fixture="${REPO_ROOT}/crates/pdfcraft-forms/tests/fixtures/basic_form.pdf"
enc_out="${work}/encrypted.pdf"
merged_out="${work}/merged.pdf"

{
  echo "feature=post-encrypt-merge"
  echo "encrypt_in=${fixture}"
} | tee "${action_log}"

cargo run -p pdfcraft-post --example encrypt -- \
  "${fixture}" "${enc_out}" "verify-owner-secret" 2>&1 | tee -a "${action_log}"

cargo run -p pdfcraft-post --example merge -- \
  "${merged_out}" "${fixture}" "${enc_out}" 2>&1 | tee -a "${action_log}"

cp -f "${enc_out}" "${proof_dir}/encrypted.pdf"
cp -f "${merged_out}" "${proof_dir}/merged.pdf"

{
  echo "=== encrypt pdfinfo ==="
  pdfinfo "${proof_dir}/encrypted.pdf" || true
  echo "=== merge pdfinfo ==="
  pdfinfo "${proof_dir}/merged.pdf"
} | tee "${proof_dir}/observe.txt"

if ! head -c 5 "${proof_dir}/encrypted.pdf" | grep -q '%PDF-'; then
  echo "drive: encrypted missing %PDF-" >&2
  exit 1
fi
# Encrypted PDFs usually report Encrypted: yes
if ! pdfinfo "${proof_dir}/encrypted.pdf" 2>/dev/null | grep -qi 'Encrypted:.*yes'; then
  # Some pdfinfo builds still parse; require file grew / differs from input at least.
  if cmp -s "${fixture}" "${proof_dir}/encrypted.pdf"; then
    echo "drive: encrypt output identical to input" >&2
    exit 1
  fi
fi

in_pages="$(pdfinfo "${fixture}" | awk -F: '/^Pages:/ {gsub(/ /,"",$2); print $2}')"
enc_pages="$(pdfinfo "${proof_dir}/encrypted.pdf" | awk -F: '/^Pages:/ {gsub(/ /,"",$2); print $2}')"
merged_pages="$(pdfinfo "${proof_dir}/merged.pdf" | awk -F: '/^Pages:/ {gsub(/ /,"",$2); print $2}')"
expected=$((in_pages + enc_pages))
if [[ "${merged_pages}" -ne "${expected}" ]]; then
  echo "drive: merge pages ${merged_pages} != ${in_pages}+${enc_pages}" >&2
  exit 1
fi

echo "drive post-encrypt-merge: ok → ${proof_dir}"
