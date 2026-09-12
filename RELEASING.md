# Temporal release contract

The authority is Sigil's P6 keyless provenance policy and its 2026-08-27
autonomous-publication amendment. The lead owns repository controls, release
dispatch and external acceptance; workers do not publish.

## Unpublished 0.1.1 stable preparation

The plugin manifest prepares **0.1.1**, not a published release. The
`publish = false` runtime crate and Cargo.lock remain at the accepted
**0.1.1-rc.1**: this is plugin-package promotion, not a Rust crate release.
Package/crate independence also preserved the component in the 0.1.0 promotion.
Only `0.1.1` and canonical positive `0.1.1-rc.N` package versions are admitted
by this checkout's packer and publisher. Historical 0.1.0 releases are never
rebuilt under their old identities. Confirm the new version is unused before
publication; a local source version or earlier availability check is not proof.

The candidate requires `sigil = ">=0.35.0"`, exact `host_api = "=1.3.0"`,
schema 4 and the unchanged `sigil:host/grpc-unary@1.3.0` import. The removed
minor ceiling does not prove compatibility with unmeasured future hosts. Keep
the public minimum-host validator pinned and record any additional measured
host versions separately. The WIT package, entrypoint and fixed Start identity
remain 0.1.0. Rebuild and measure the component before claiming byte identity;
a crate-version bump alone is not proof either way.

The local development manifest/packer remains a separate, historical
`0.1.0-dev.N` path. Its 0.34.0 source-build requirement is not the requirement
for official 0.1.1 packages. No local package acquires publication authority.

All following independent review, provenance, immutability and acquisition
gates remain required. Preparing this source does not authorize dispatch.

The official locked **0.1.1-rc.1** and exact companion from source
`103aed13cdc16f2d5e691ea16244e2d8f3560f4a` passed CAPI acceptance on
**2026-09-11**, using Sigil **0.35.1**: five profiles, ten scenarios, 319
unchanged assertions and both frozen expected-RED fingerprints. Its
**2026-09-12** addendum closed diagnostic evidence without a full-suite rerun.
The caller is independently reviewed but deliberately held for stable under
CAPI's merge policy. That does not imply a missing RC service-acceptance gate
or an already-merged caller.

Stable candidate and public readback MUST retain component BLAKE3
`b427ab70cb4643c771996a3610456872e4ed50e49f24fe8859186a654833a7b8`.
Any different component stops this promotion; investigate and seek a new
review/acceptance decision, never relabel old evidence. Keep the accepted
runtime, helper, WIT, dependency locks and build settings unchanged and measure
the actual build. The stable manifest/package and source-bound sidecar acquire
new identities and still require exact-candidate review and public verification.

Keep the public **0.35.0** minimum validator and its hashes pinned; the measured
0.35.1 host does not change the manifest floor. The Lua companion is project-side
source copied explicitly by callers, not an added file inside the two-member
plugin archive; CAPI must record the helper source revision when adopting it.

The 0.1.1 release packer also selects zstd `--single-thread` explicitly.
Sigil's writer uses a single-threaded streaming encoder; the CLI's default
one-worker mode can yield different compressed bytes for the same multi-block
tar. Both forms can pass package validation, so validator acceptance is not
proof of writer byte parity. The large-component regression compares the
release packer with the pinned public Sigil writer, checks repeatability and
refuses output reuse. Historical local-development packaging is unchanged.
This correction changes new package identity, not existing published assets.

## Before the candidate build

- Source and whole feature range independently reviewed, no open blockers.
- Public supporting Sigil 0.35.0 exists, with independently measured archive and
  extracted executable checksums pinned in `scripts/release-tools.json`. A local
  binary with that version text does not establish public release identity.
  Acquisition checks the executable pin before exposing it, and release
  validators check the same immutable expected digest before and after every
  invocation. Do not derive the expected digest from whichever binary happens
  to be at the path after builds. This is checked executable identity, not an
  isolation claim against arbitrary concurrent same-user compromise.
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
When the project already has a lock, sync it first to populate its existing
dependencies before adding the intended version. Add resolves the whole lock;
omit add if it already selects that version. Apply the same acquisition order
when verifying the stable package, without substituting its RC package identity.
Explicit install-first separates acquisition evidence for auditing and retains
compatibility with older hosts; it is not a limitation of Sigil 0.35.0, whose
`plugin add` can acquire a missing package through verified remote installation.
Verify source commit and every lock digest against the approved tuple. Real
CAPI acceptance uses ordinary `sigil run`, unchanged assertions and exact
expected-RED fingerprints with pinned service/rig identities. No non-gating
`plugin test` or local-source policy exception substitutes for that gate.

For any accepted RC, stable promotion is a new independently reviewed candidate
and publication. Promotion does not mutate the immutable prerelease or claim
its package digest is unchanged when the manifest version changes.

The official locked 0.1.0-rc.1 received CAPI caller-replacement acceptance on
2026-09-10 (five profiles, ten scenarios, 319 unchanged assertions, both exact
expected-RED fingerprints). This closes the RC service-acceptance prerequisite,
not a new package's exact-candidate review or publication gate. Historical
0.1.0 stable preparation required the accepted component BLAKE3
`b25139ed2e6eeab88ed26f8e306621cd83869670084f986e481143b57acc372d`;
retain each new package/manifest identity separately. The operator/caller
requirements that enabled acceptance are documented in the README.
