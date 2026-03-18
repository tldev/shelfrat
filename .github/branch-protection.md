# Branch Protection — main

Apply these settings via **Settings → Branches → Add rule** for `main`:

## Required status checks (must pass before merge)

- `Backend lint`
- `Backend tests`
- `Frontend lint`
- `Frontend tests`
- `Cargo audit`
- `npm audit`
- `Trivy vulnerability scan`
- `CodeQL analysis`
- `Docker build`

## Recommended settings

- [x] Require a pull request before merging
- [x] Require status checks to pass before merging
- [x] Require branches to be up to date before merging
- [x] Do not allow bypassing the above settings
