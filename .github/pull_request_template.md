## What and why
<!-- What does this change for the user, and why? Link the ROADMAP line,
memory/features.json id, issue or spec. -->

## How it was verified
<!-- Paste the result lines. "Tests pass" without output is not evidence. -->
- [ ] `make check` passes
- [ ] New or changed behaviour has a test that fails on the old code
- [ ] Roof logic: DACAD fixture or measured case named; no fixture output changed
- [ ] UI: preview screenshot; all four languages; `make sweeps` clean
- [ ] MCP parity: the MCP tool exists and `guide.md` / `tectora-roof` skill updated

```
<make check output>
```

## Review
- [ ] `tectora-review` run on the diff; verified S1/S2 fixed or listed here
- Remaining S3/S4 or unverified items:

## Risk
- [ ] No DACAD material, no secrets, no weakened tests, no unexplained new dependency
- [ ] Older `.tectora` files still open (format changes only)
- macOS / Windows notes:
