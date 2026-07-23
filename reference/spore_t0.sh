#!/usr/bin/env bash
# SPORE Tier-0 reference node in shell — parse, address, ID, and verify, using
# only ubiquitous Unix tools (sha256sum, base32, openssl). The smallest useful
# node for a box that has a shell but no Rust or Python: enough to receive and
# trust public messages, and a cross-language conformance oracle for
# docs/REBUILD.md.
#
#   reference/spore_t0.sh <hex-or-armor>
#   echo '~S1.….~' | reference/spore_t0.sh
#
# Ed25519 verification is delegated to openssl (3.0+, one-shot -rawin); the raw
# 32-byte public key is wrapped in its 12-byte DER SubjectPublicKeyInfo prefix.
set -eu

# hex string -> raw bytes on stdout.
h2b() { printf %b "$(printf %s "$1" | sed 's/../\\x&/g')"; }
# sha256 of the bytes for hex string $1 -> hex digest.
sha() { h2b "$1" | sha256sum | cut -d' ' -f1; }

# Read input (argument or stdin), trim whitespace.
raw="${1:-$(cat)}"
raw="$(printf %s "$raw" | tr -d '[:space:]')"

# Armor (~S1.<base32>.<base32 checksum>~) -> wire hex.
case "$raw" in
  '~S1.'*)
    body="${raw#\~S1.}"; body="${body%\~}"
    b32="${body%.*}"; ck="${body##*.}"
    # base32 needs uppercase + '=' padding to a multiple of 8.
    pad() { local s="$1"; while [ $(( ${#s} % 8 )) -ne 0 ]; do s="${s}="; done; printf %s "$s"; }
    wirehex="$(printf %s "$(pad "$b32")" | base32 -d 2>/dev/null | od -An -tx1 | tr -d ' \n')"
    cksum="$(printf %s "$(pad "$ck")" | base32 -d 2>/dev/null | od -An -tx1 | tr -d ' \n')"
    want="$(sha "$wirehex" | cut -c1-8)"
    [ "$cksum" = "$want" ] || { echo "armor checksum mismatch" >&2; exit 1; }
    wire="$wirehex"
    ;;
  *) wire="$(printf %s "$raw" | tr 'A-F' 'a-f')" ;;
esac

hexlen=${#wire}
[ $(( hexlen % 2 )) -eq 0 ] || { echo "odd-length hex" >&2; exit 1; }
byte() { printf %s "${wire:$(( $1 * 2 )):2}"; }   # byte at index -> 2 hex chars
slice() { printf %s "${wire:$(( $1 * 2 )):$(( $2 * 2 ))}"; }  # from,len (bytes)

ver=$(byte 0); typ=$(byte 1); flags=$(byte 2); hops=$(byte 3)
[ "$ver" = "01" ] || { echo "not a SPORE v1 envelope (ver=$ver)" >&2; exit 1; }
fl=$(( 16#$flags ))
signed=$(( fl & 2 )); src8=$(( fl & 32 ))

expiry=$(( 16#$(slice 4 4) ))
dest=$(slice 8 8)
off=16
pubkey=""
if [ "$signed" -ne 0 ]; then
  if [ "$src8" -ne 0 ]; then off=$(( off + 8 ));
  else pubkey=$(slice "$off" 32); off=$(( off + 32 )); fi
fi
plen=$(( 16#$(slice "$off" 2) )); off=$(( off + 2 ))
payload=$(slice "$off" "$plen"); off=$(( off + plen ))

# ID = SHA-256(wire with hops byte zeroed)[..16].
idwire="${wire:0:6}00${wire:8}"
id="$(sha "$idwire" | cut -c1-32)"

typename() { case "$1" in 00) echo DATA;; 01) echo INV;; 02) echo WANT;; 03) echo ANNOUNCE;; *) echo "$1";; esac; }
echo "type    : $(typename "$typ")"
echo "flags   : 0x$flags"
echo "hops    : $(( 16#$hops ))"
echo "expiry  : $expiry"
echo "dest    : $dest"
if [ -n "$pubkey" ]; then
  echo "src key : $pubkey"
  echo "src addr: $(sha "$pubkey" | cut -c1-16)"
fi
echo "id      : $id"
printf 'payload : '; h2b "$payload" 2>/dev/null | tr -d '\000' ; echo

# Verify the signature via openssl, if signed with a full key.
if [ -n "$pubkey" ]; then
  sig="${wire:$(( (hexlen/2 - 64) * 2 ))}"                 # last 64 bytes
  body="${wire:0:$(( (hexlen/2 - 64) * 2 ))}"             # wire without signature
  body="${body:0:6}00${body:8}"                          # hops byte zeroed
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
  h2b "302a300506032b6570032100${pubkey}" > "$tmp/pub.der"
  openssl pkey -pubin -inform DER -in "$tmp/pub.der" -out "$tmp/pub.pem" 2>/dev/null
  h2b "$body" > "$tmp/msg.bin"; h2b "$sig" > "$tmp/sig.bin"
  if openssl pkeyutl -verify -pubin -inkey "$tmp/pub.pem" -rawin \
       -in "$tmp/msg.bin" -sigfile "$tmp/sig.bin" >/dev/null 2>&1; then
    echo "signature verifies: True"
  else
    echo "signature verifies: False"
  fi
fi
