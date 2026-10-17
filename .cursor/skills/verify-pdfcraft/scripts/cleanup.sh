#!/usr/bin/env bash
# Tear down scratch/state for this run. Keeps evidence/proof artifacts.
set -euo pipefail
# shellcheck source=env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/env.sh"

# No long-lived server. Only scratch dirs and optional lock files.
if [[ -d "${SCRATCH_DIR}" ]]; then
  rm -rf "${SCRATCH_DIR}"
  echo "cleanup: removed ${SCRATCH_DIR}"
fi

lock="${REPO_ROOT}/out/.verify-equity.lock"
if [[ -f "${lock}" ]]; then
  # Only remove if we can lock it (not held); never kill by process name.
  exec 9>"${lock}"
  if flock -n 9; then
    rm -f "${lock}"
    echo "cleanup: removed idle ${lock}"
  else
    echo "cleanup: lock held; left ${lock}"
  fi
fi

rm -f "${RUN_DIR}/.launched"
echo "cleanup: evidence kept at ${EVIDENCE_DIR}"
ls -la "${EVIDENCE_DIR}" 2>/dev/null || echo "cleanup: no evidence dir (drive may not have run)"
