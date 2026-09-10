# Temporal release contract

The authority is Sigil's P6 keyless provenance policy and its 2026-08-27
autonomous-publication amendment. The lead owns repository controls, release
dispatch and external acceptance; workers do not publish.

## Before the candidate build

- Source and whole feature range independently reviewed, no open blockers.
- Public supporting Sigil 0.35.0 exists, with exact archive checksum pinned in
  `scripts/release-tools.json`. A local binary with that version text does not
  establish public release identity.
- GitHub immutable releases enabled and owner-enforced; protected main and
  release environment restricted to main. Read controls back through the admin
  API; retain response identity/time with the approval evidence.
- Source commit, manifest version and intended official repository agree.
- Exact environment and tools recorded; Rust 1.95.0, wasm-tools 1.252.0,
  just 1.57.0, b3sum 1.8.3 and zstd 1.5.7. Dependency acquisition uses lockfiles;
  verification/build commands remain offline after that explicit acquisition.

## Candidate and independent approval

Dispatch `prepare-release` on main. Retain the successful first-attempt run ID,
source commit, workflow identity, three downloaded assets and their SHA-256s.
Record the WASM component hash too. The P6 manifest is canonical JSON without a
trailing newline and carries package SHA-256 and package/manifest/component
BLAKE3 identities. The checksum file contains exactly the one package line.

Revalidate the downloaded package with the exact supporting public Sigil binary
and review the full candidate workflow logs. Linux and Darwin host qualification
remain separate evidence from the portable component conformance harness.
Tests and package validation do not claim real Temporal service compatibility.

Independent approval must name the exact source commit, candidate run ID,
SemVer, package SHA-256, SHA256SUMS SHA-256 and release-manifest.json SHA-256.
Freshly read back immutable-release controls immediately before publication.
Dispatch `publish-release` only on that exact main commit and tuple. It runs
once, serializes publications and never builds or replaces candidate bytes.

## Failure and promotion

Any failed check stops publication. A failed or rerun candidate is not an
approved first-attempt candidate. Preserve its logs and use a newly reviewed
candidate run; never delete failed evidence or represent a retry as attempt 1.
An existing tag, draft/release, or attestation burns that version: use a new
SemVer after investigating, not an overwrite. If publication fails after an
external write, preserve the partial state and report it; do not rerun the
publication job or erase a draft/tag/attestation to reuse the identity.

The public RC must be discoverable by exact version, installed from an empty
cache with official-github-provenance-v1, then added to a project lock and synced.
Verify source commit and every lock digest against the approved tuple. Real
CAPI acceptance uses ordinary `sigil run`, unchanged assertions and exact
expected-RED fingerprints with pinned service/rig identities. No non-gating
`plugin test` or local-source policy exception substitutes for that gate.

After acceptance, stable 0.1.0 is a new independently reviewed candidate and
publication. Promotion does not mutate the immutable prerelease or claim its
package digest is unchanged when the manifest version changes.
