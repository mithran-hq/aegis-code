#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

required_files=(
  README.md
  LICENSE
  NOTICE
  AGENTS.md
  CONTRIBUTING.md
  SECURITY.md
  CODE_OF_CONDUCT.md
  docs/ARCHITECTURE.md
  docs/DISTRIBUTION.md
  docs/IMPLEMENTATION_PLAN.md
  docs/adr/0001-aegis-code-product-boundary.md
  .github/ISSUE_TEMPLATE/plan.yml
  .github/ISSUE_TEMPLATE/task.yml
)

for file in "${required_files[@]}"; do
  if [[ ! -s "$file" ]]; then
    echo "missing required file: $file" >&2
    exit 1
  fi
done

if grep -R "bruno-gate\\|Bruno Gate" README.md AGENTS.md .github 2>/dev/null; then
  echo "found stale bruno-gate naming in product surfaces" >&2
  exit 1
fi

echo "scaffold checks passed"
