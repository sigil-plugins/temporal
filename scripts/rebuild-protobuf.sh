#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C
unset CDPATH
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
MODE="${1:---check}"
case "$MODE" in
  --verify|--check|--write) ;;
  *) echo 'usage: scripts/rebuild-protobuf.sh [--verify|--check|--write]' >&2; exit 2 ;;
esac
SHA256_TOOL="${PROTOBUF_SHA256_TOOL:-auto}"
if [[ "$SHA256_TOOL" == auto ]]; then
  if command -v sha256sum >/dev/null 2>&1; then SHA256_TOOL=sha256sum; else SHA256_TOOL=shasum; fi
fi
case "$SHA256_TOOL" in
  sha256sum) SHA256=(sha256sum) ;;
  shasum) SHA256=(shasum -a 256) ;;
  *) echo 'PROTOBUF_SHA256_TOOL must be auto, sha256sum, or shasum' >&2; exit 2 ;;
esac
command -v "${SHA256[0]}" >/dev/null 2>&1 || {
  echo 'SHA-256 verification requires sha256sum or shasum (Perl Digest::SHA)' >&2; exit 1;
}
inventory_hashes() {
  while IFS= read -r file; do "${SHA256[@]}" -- "$file"; done
}

# Inventory verification has no network, compiler, protoc, or codegen dependency.
# Verify exact path sets as well as hashes; an unlisted file is not trusted input.
verify_inventory() {
  local directory="$1"
  (
    cd "$directory"
    test -f SHA256SUMS
    test -z "$(find . -type l -print)"
    diff -u <(sed 's/^[0-9a-f]*  //' SHA256SUMS | sort) \
      <(find . -type f ! -name SHA256SUMS -print | sed 's|^\./||' | sort)
    "${SHA256[@]}" --check --strict SHA256SUMS >/dev/null
  )
}
verify_inventory vendor
verify_inventory conformance/requests
if [[ "$MODE" == --verify ]]; then
  verify_inventory src/generated
  verify_inventory conformance/responses
  echo 'protobuf source, generated-code and oracle inventories verified (no generation)'
  exit 0
fi

PROTOC="${PROTOC:-protoc}"
export PROTOC
[[ "$("$PROTOC" --version)" == 'libprotoc 35.1' ]] || {
  echo 'regeneration requires exactly libprotoc 35.1' >&2; exit 1;
}
mkdir -p target
STAGE="$(mktemp -d "$ROOT/target/protobuf.XXXXXXXX")"
cleanup() {
  case "$STAGE" in "$ROOT"/target/protobuf.*) rm -rf -- "$STAGE" ;; esac
}
trap cleanup EXIT
mkdir -p "$STAGE/generated" "$STAGE/requests" "$STAGE/responses/source"
cargo run --quiet --locked --offline --manifest-path tools/codegen/Cargo.toml -- \
  vendor/temporal-api "$STAGE/generated"
rustfmt --edition 2024 "$STAGE/generated/wire.rs"
(
  cd "$STAGE/generated"
  find . -type f ! -name SHA256SUMS -print | sed 's|^\./||' | sort | inventory_hashes > SHA256SUMS
)

encode() {
  local source="$1" destination="$2" kind="$3"
  "$PROTOC" --proto_path=vendor/temporal-api --encode="temporal.api.workflowservice.v1.$kind" \
    temporal/api/workflowservice/v1/request_response.proto < "$source" > "$destination"
}
for source in conformance/requests/source/*.textproto; do
  name="$(basename "$source" .textproto)"
  case "$name" in
    start-*) kind=StartWorkflowExecutionRequest ;;
    describe-*) kind=DescribeWorkflowExecutionRequest ;;
    history-*) kind=GetWorkflowExecutionHistoryRequest ;;
    *) echo "unknown request fixture $source" >&2; exit 1 ;;
  esac
  encode "$source" "$STAGE/requests/$name.pb" "$kind"
  # The four imported Sigil request oracles are immutable in this repository.
  cmp "conformance/requests/$name.pb" "$STAGE/requests/$name.pb"
done
cp conformance/responses/source/* "$STAGE/responses/source/"
cp conformance/responses/README.md "$STAGE/responses/README.md"
for source in "$STAGE"/responses/source/*.textproto; do
  name="$(basename "$source" .textproto)"
  case "$name" in
    start-*) kind=StartWorkflowExecutionResponse ;;
    describe-*) kind=DescribeWorkflowExecutionResponse ;;
    history-*) kind=GetWorkflowExecutionHistoryResponse ;;
    future-fields|started-false) continue ;;
    *) echo "unknown response fixture $source" >&2; exit 1 ;;
  esac
  encode "$source" "$STAGE/responses/$name.pb" "$kind"
done
# Official proto3 encoding erases the distinction between omitted and false.
cmp "$STAGE/responses/start-omitted.pb" "$STAGE/responses/start-false.pb"
# A test-only proto2 presence fragment emits the legal explicit-false wire
# representation; it is not an independent oracle for Temporal field numbers.
"$PROTOC" --proto_path=conformance/responses/source \
  --encode=sigil.temporal.conformance.StartedPresence started-presence.proto \
  < conformance/responses/source/started-false.textproto > "$STAGE/started-false.pb"
cat "$STAGE/responses/start-omitted.pb" "$STAGE/started-false.pb" \
  > "$STAGE/responses/start-wire-false.pb"
# A separately compiled synthetic future field tests protobuf forward
# compatibility. It does not redefine or copy any official message field tag.
"$PROTOC" --proto_path=conformance/responses/source \
  --encode=sigil.temporal.conformance.FutureFields future-fields.proto \
  < conformance/responses/source/future-fields.textproto > "$STAGE/future-fields.pb"
cat "$STAGE/responses/history-future-event-page-two.pb" "$STAGE/future-fields.pb" \
  > "$STAGE/responses/history-future-fields.pb"
(
  cd "$STAGE/responses"
  find . -type f ! -name SHA256SUMS -print | sed 's|^\./||' | sort | inventory_hashes > SHA256SUMS
)
if [[ "$MODE" == --write ]]; then
  mkdir -p src/generated
  # Never silently remove an obsolete/unlisted generated file: --verify fails
  # until its removal is explicitly reviewed alongside the regeneration.
  cp "$STAGE"/generated/* src/generated/
  cp "$STAGE"/responses/*.pb "$STAGE/responses/SHA256SUMS" conformance/responses/
else
  verify_inventory src/generated
  verify_inventory conformance/responses
  diff -r "$STAGE/generated" src/generated
  diff -r "$STAGE/responses" conformance/responses
fi
echo 'pinned message-only codegen and independent protoc oracles match'
