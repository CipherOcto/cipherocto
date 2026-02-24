## Pull Request Checklist

- [ ] All CI checks pass (green ✓)
- [ ] Lint checks pass (no formatting issues)
- [ ] Security scan passes
- [ ] Tests added/updated for new functionality
- [ ] Documentation updated (if applicable)
- [ ] Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)

---

## Type of Change

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update
- [ ] Refactoring (no functional changes)
- [ ] AI-generated work (`agent/*` branch)

---

## Branch Strategy

This PR follows the CipherOcto branch strategy:

| From Branch | To Branch | Purpose |
|-------------|-----------|---------|
| `feat/*` | `next` | New features |
| `agent/*` | `next` | AI-generated code |
| `research/*` | `next` | Experimental work |
| `hotfix/*` | `main` | Emergency fixes |
| `next` | `main` | Integration release |

**Current PR:** `<!-- source branch -->` → `<!-- target branch -->`

---

## Description

<!-- Describe your changes in detail -->

---

## Related Issues

Closes #(issue)

---

## Testing

<!-- Describe how you tested your changes -->

- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Manual testing performed

---

## Performance Impact

- [ ] No performance impact
- [ ] Performance improved (describe)
- [ ] Performance degraded (describe, justify)

---

## Security Considerations

- [ ] No security implications
- [ ] Security changes (describe)

<!-- For blockchain/crypto changes, consider:
- Private key handling
- Signature verification
- Access control
- Input validation
-->

---

## Additional Notes

<!-- Any other context for reviewers -->
