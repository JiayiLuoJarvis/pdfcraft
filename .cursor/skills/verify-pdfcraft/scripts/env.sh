#!/usr/bin/env bash
# Shared env for verify-pdfcraft helpers. Source from other scripts; do not run alone.
set -euo pipefail

VERIFY_SKILL_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "${VERIFY_SKILL_ROOT}/../../.." && pwd)"
cd "${REPO_ROOT}"

RUN_ID="${VERIFY_RUN_ID:-$(date +%Y%m%dT%H%M%S)-$$}"
RUN_DIR="${VERIFY_RUN_DIR:-${VERIFY_SKILL_ROOT}/evidence/${RUN_ID}}"
SCRATCH_DIR="${RUN_DIR}/scratch"
EVIDENCE_DIR="${RUN_DIR}/proof"
mkdir -p "${SCRATCH_DIR}" "${EVIDENCE_DIR}"

resolve_font() {
  local candidate
  if [[ -n "${PDFCRAFT_CJK_FONT:-}" && -f "${PDFCRAFT_CJK_FONT}" ]]; then
    printf '%s\n' "${PDFCRAFT_CJK_FONT}"
    return 0
  fi
  if [[ -n "${PDFCRAFT_PDF_FONT:-}" && -f "${PDFCRAFT_PDF_FONT}" ]]; then
    printf '%s\n' "${PDFCRAFT_PDF_FONT}"
    return 0
  fi
  for candidate in \
    "${REPO_ROOT}/.fonts/body.ttf" \
    /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf; do
    if [[ -f "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done
  return 1
}

resolve_forms_font() {
  local candidate
  if [[ -n "${PDFCRAFT_FORMS_FONT:-}" && -f "${PDFCRAFT_FORMS_FONT}" ]]; then
    printf '%s\n' "${PDFCRAFT_FORMS_FONT}"
    return 0
  fi
  for candidate in \
    /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf \
    "${REPO_ROOT}/.fonts/body.ttf"; do
    if [[ -f "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done
  return 1
}

export_fonts() {
  local layout_font forms_font
  layout_font="$(resolve_font)"
  forms_font="$(resolve_forms_font)"
  export PDFCRAFT_PDF_FONT="${PDFCRAFT_PDF_FONT:-${layout_font}}"
  export PDFCRAFT_CJK_FONT="${PDFCRAFT_CJK_FONT:-${layout_font}}"
  export PDFCRAFT_ORGCHART_FONT="${PDFCRAFT_ORGCHART_FONT:-${layout_font}}"
  export PDFCRAFT_FORMS_FONT="${PDFCRAFT_FORMS_FONT:-${forms_font}}"
}

export VERIFY_SKILL_ROOT REPO_ROOT RUN_ID RUN_DIR SCRATCH_DIR EVIDENCE_DIR
