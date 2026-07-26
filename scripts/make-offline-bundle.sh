#!/bin/sh
# Vendor every Rust dependency into the tree so the repo builds with no registry
# and no network — the "one laptop, no internet" case in docs/CONTINUITY.md.
#
# Run this ONCE while you still have a network. After it, `cargo build` works on
# a machine that has never seen crates.io: Cargo.lock pins the versions, and
# vendor/ holds the actual sources those versions resolve to. A clone alone does
# not — the lockfile names the crates, it does not contain them.
#
#   ./scripts/make-offline-bundle.sh          # vendor in place
#   ./scripts/make-offline-bundle.sh --tar    # ...and a signed-able tarball
#
# vendor/ is deliberately not committed: it is ~10 MB of third-party source, and
# the repo stays small enough to carry on the media CONTINUITY.md describes. This
# script is how you turn a clone into an offline-buildable one on demand.
set -eu

cd "$(dirname "$0")/.."

echo "vendoring dependencies into vendor/ ..."
cargo vendor --versioned-dirs vendor >/tmp/spore-vendor-config.$$ 2>/dev/null ||
	cargo vendor --versioned-dirs vendor >/tmp/spore-vendor-config.$$

# `cargo vendor` prints the config that activates the vendored sources; without
# it Cargo ignores vendor/ entirely and still reaches for the registry.
mkdir -p .cargo
cat /tmp/spore-vendor-config.$$ >.cargo/config.toml
rm -f /tmp/spore-vendor-config.$$

echo "verifying the build needs no registry ..."
cargo build --offline --quiet
echo "  ok — cargo build --offline succeeds"

if [ "${1:-}" = "--tar" ]; then
	version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
	out="spore-offline-${version}.tar.gz"
	# Everything a cold machine needs: the source, the pins, the sources the
	# pins name, and the config that points Cargo at them.
	tar --exclude=./target --exclude=./.git -czf "../$out" .
	mv "../$out" .
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$out" >"$out.sha256"
	elif command -v shasum >/dev/null 2>&1; then
		shasum -a 256 "$out" >"$out.sha256"
	fi
	echo "  wrote $out"
	[ -f "$out.sha256" ] && cat "$out.sha256"
fi

echo
echo "This clone now builds offline. vendor/ and .cargo/config.toml are"
echo "gitignored — they are build inputs for this machine, not repo content."
