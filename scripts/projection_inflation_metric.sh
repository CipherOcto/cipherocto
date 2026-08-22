#!/usr/bin/env bash
# scripts/projection_inflation_metric.sh
# Guard 6: Projection-based decomposition inflation counter-metric (per long-horizon plan v1.3 Phase 2 §Guard 6; M37 NEW P3)
# Behavior: For each review round, compare projection-based count vs corpus STATE audit count.
#           Reject round close if inflation > 50% (less than 50% realization = false-positive inflation).
# Coverage: All future review rounds.
# 3-instance dataset: R30 13.5% / M34 40% / M37 46%.

set -euo pipefail

usage() {
    cat <<EOF
Usage: $0 <projection_count> <state_audit_count>

Example:
    $0 200 70
    # Projection said 200 issues; corpus STATE audit found 70.
    # Realization rate: 70/200 = 35% (below 50% threshold).

Compares projection-based decomposition counts vs corpus STATE audit counts.
Rejects round close if inflation > 50% (less than 50% realization = false-positive inflation).

Per M37 realization-rate pattern (R30 13.5% / M34 40% / M37 46%), corpus STATE audit MUST
be the canonical count for all future review tasks. Projection-based counts systematically
inflate findings by 2x-7x.

Threshold: REALIZATION_RATE >= 50% (i.e., STATE_AUDIT / PROJECTION >= 0.5)
EOF
    exit 1
}

if [ $# -ne 2 ]; then
    usage
fi

projection="$1"
state_audit="$2"

echo "Projection inflation counter-metric — Guard 6 (M37 NEW P3)"
echo ""
echo "Projection count (task-brief estimate): $projection"
echo "Corpus STATE audit count (on-disk parser): $state_audit"

if [ "$projection" -eq 0 ]; then
    echo "Skipping: projection count is 0 (no inflation possible)."
    exit 0
fi

# Compute realization rate (state_audit / projection)
realization=$(awk -v p="$projection" -v s="$state_audit" 'BEGIN { printf "%.1f", (s / p) * 100 }')
inflation=$(awk -v p="$projection" -v s="$state_audit" 'BEGIN { printf "%.1f", ((p - s) / p) * 100 }')

echo ""
echo "Realization rate: ${realization}%"
echo "Inflation rate: ${inflation}%"

# Threshold: realization >= 50%
threshold=50
threshold_met=$(awk -v r="$realization" -v t="$threshold" 'BEGIN { print (r >= t) ? 1 : 0 }')

echo ""
echo "Threshold: realization_rate >= ${threshold}%"
echo "Result: $([ "$threshold_met" -eq 1 ] && echo 'PASS' || echo 'FAIL')"

if [ "$threshold_met" -eq 0 ]; then
    echo ""
    echo "FAIL: projection inflation > 50% (realization rate ${realization}% < ${threshold}%)."
    echo ""
    echo "Per M37 lesson: corpus STATE audit MUST be canonical for review counts."
    echo "Fix: re-baseline round with corpus STATE audit; projection-based decomposition"
    echo "     systematically inflates findings (R30 13.5% / M34 40% / M37 46% historical)."
    exit 1
fi

echo ""
echo "PASS: round close authorized."
exit 0
