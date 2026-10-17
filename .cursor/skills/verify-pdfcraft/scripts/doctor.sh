#!/usr/bin/env bash
# Read-only: is this checkout worth driving?
set -euo pipefail
# shellcheck source=env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/env.sh"

ok=0
fail=0
check() {
  local label="$1"
  shift
  if "$@"; then
    printf 'OK   %s\n' "${label}"
    ok=$((ok + 1))
  else
    printf 'FAIL %s\n' "${label}"
    fail=$((fail + 1))
  fi
}

check "repo Cargo.toml" test -f "${REPO_ROOT}/Cargo.toml"
check "workspace member pdfcraft-pdf" test -d "${REPO_ROOT}/crates/pdfcraft-pdf"
check "cargo on PATH" bash -c 'command -v cargo >/dev/null'
check "rustc on PATH" bash -c 'command -v rustc >/dev/null'
check "pdftotext on PATH" bash -c 'command -v pdftotext >/dev/null'
check "pdfinfo on PATH" bash -c 'command -v pdfinfo >/dev/null'

if layout_font="$(resolve_font)"; then
  printf 'OK   layout font → %s\n' "${layout_font}"
  ok=$((ok + 1))
else
  printf 'FAIL layout font (set PDFCRAFT_PDF_FONT / PDFCRAFT_CJK_FONT or provide .fonts/body.ttf)\n'
  fail=$((fail + 1))
fi

if forms_font="$(resolve_forms_font)"; then
  printf 'OK   forms font → %s\n' "${forms_font}"
  ok=$((ok + 1))
else
  printf 'FAIL forms font (set PDFCRAFT_FORMS_FONT)\n'
  fail=$((fail + 1))
fi

check "forms fixture basic_form.pdf" \
  test -f "${REPO_ROOT}/crates/pdfcraft-forms/tests/fixtures/basic_form.pdf"

printf '\nrustc: %s\ncargo: %s\nrun-id: %s\n' \
  "$(rustc --version)" "$(cargo --version)" "${RUN_ID}"

if [[ "${fail}" -ne 0 ]]; then
  printf '\ndoctor: %s ok, %s failed — do not drive\n' "${ok}" "${fail}"
  exit 1
fi
printf '\ndoctor: %s ok — ready to drive\n' "${ok}"
