## Outcome

What is easier to understand or safer to change? Separate deliberate behavior
changes from behavior-preserving extraction and file moves.

## Maintainability review

- [ ] Names expose domain roles; helpers own cohesive responsibilities.
- [ ] Shared mechanisms preserve each consumer's policy and refusal order.
- [ ] Validation remains at the live mutation boundary; no cached permission.
- [ ] Input precedence, handled no-ops, history/revisions and publication timing
      are preserved (or explicitly identified as changed).
- [ ] Ownership, dependency direction, public paths and feature boundaries hold.
- [ ] No global rename, arbitrary line-limit split or broad lint suppression.

## Evidence

List exact affected tests/gates, baseline characterizations, ignored probes and
remaining risks. Include paint/access/pixel comparisons for projection changes,
and cost evidence for allocation/shaping/virtualization changes as appropriate.
Do not substitute a passing test count for invariants or platform certification.

See [CONTRIBUTING.md](../CONTRIBUTING.md) for contracts and local verification.
