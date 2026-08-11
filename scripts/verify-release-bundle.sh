#!/bin/sh
set -eu

bundle_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
expected=$(mktemp "${TMPDIR:-/tmp}/openjoc-release-expected.XXXXXX")
actual=$(mktemp "${TMPDIR:-/tmp}/openjoc-release-actual.XXXXXX")
cleanup() {
    rm -f -- "$expected" "$actual"
}
trap cleanup EXIT HUP INT TERM

cat > "$expected" <<'EOF'
KNOWN_LIMITATIONS.md
LICENSE
README.md
RELEASE_MANIFEST.json
SHA256SUMS
bin/openjoc
verify.sh
EOF

(
    cd "$bundle_root"
    find . -type f -print | sed 's#^\./##' | LC_ALL=C sort > "$actual"
)

if ! cmp -s "$expected" "$actual"; then
    echo "openjoc release verification: exact file inventory mismatch" >&2
    diff -u "$expected" "$actual" >&2 || true
    exit 1
fi

for required in \
    KNOWN_LIMITATIONS.md \
    LICENSE \
    README.md \
    RELEASE_MANIFEST.json \
    SHA256SUMS \
    bin/openjoc \
    verify.sh
do
    if [ -L "$bundle_root/$required" ]; then
        echo "openjoc release verification: symlink not allowed: $required" >&2
        exit 1
    fi
done

(
    cd "$bundle_root"
    shasum -a 256 -c SHA256SUMS
)

manifest="$bundle_root/RELEASE_MANIFEST.json"
grep -Fq '"schema": "openjoc.bundle-manifest.v1"' "$manifest"
grep -Fq '"version": "0.1.0"' "$manifest"
grep -Fq '"target": "aarch64-apple-darwin"' "$manifest"
grep -Fq '"developer_identity_signed": false' "$manifest"
grep -Fq '"linker_adhoc_signed": true' "$manifest"
grep -Fq '"notarized": false' "$manifest"

while read -r digest relative; do
    relative=${relative#\*}
    grep -Fq "\"sha256\": \"$digest\"" "$manifest"
    grep -Fq "\"path\": \"$relative\"" "$manifest"
done < "$bundle_root/SHA256SUMS"

case $(uname -s) in
    Darwin) ;;
    *)
        echo "openjoc release verification: this candidate is admitted only on macOS" >&2
        exit 1
        ;;
esac
case $(uname -m) in
    arm64) ;;
    *)
        echo "openjoc release verification: expected arm64 host" >&2
        exit 1
        ;;
esac

help_output=$($bundle_root/bin/openjoc --help)
case "$help_output" in
    "OpenJOC 0.1.0"*) ;;
    *)
        echo "openjoc release verification: binary version/help mismatch" >&2
        exit 1
        ;;
esac

for command in inspect decode decode-payload diagnose-tools census diagnose-oamd; do
    "$bundle_root/bin/openjoc" "$command" --help >/dev/null
done

echo "openjoc release verification: PASS"
