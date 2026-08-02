#!/usr/bin/env bash
# Shared resolv.conf rewrite for all template images (plain + *_dini).
#
# Contract (see .scratch/docker-in-instance_2, ID 6 "DNS under runsc"):
#  - No-op unless OW_DNS is set to a non-empty, comma-separated resolver list.
#  - Otherwise replaces /etc/resolv.conf with one `nameserver <ip>` line per
#    resolver in OW_DNS. Must run as root, before any privilege drop, so the
#    in-instance services (and, for DinI, the nested dockerd) inherit it.
#  - Exits non-zero if OW_DNS is set but the rewrite cannot be done: under
#    runsc an unrewritten resolv.conf cannot resolve, so failing is preferred
#    to silently starting with broken DNS.
set -euo pipefail

dns="${OW_DNS:-}"
if [ -z "$dns" ]; then
    echo "apply-ow-dns: OW_DNS unset/empty; leaving /etc/resolv.conf untouched."
    exit 0
fi

resolv_conf="/etc/resolv.conf"
new="${resolv_conf}.ow-dns"
trap 'rm -f "$new"' EXIT

if [ ! -w "$resolv_conf" ]; then
    echo "apply-ow-dns: ${resolv_conf} is not writable (must run as root)" >&2
    exit 1
fi

: > "$new"

# Split on commas and whitespace so "8.8.8.8, 1.1.1.1" is tolerated; empty
# fields from stray separators are skipped.
IFS=', ' read -ra resolvers <<< "$dns"
for resolver in "${resolvers[@]}"; do
    [ -n "$resolver" ] || continue
    if ! [[ "$resolver" =~ ^[0-9A-Fa-f:.]+$ ]]; then
        echo "apply-ow-dns: \"${resolver}\" in OW_DNS is not a valid IP" >&2
        exit 1
    fi
    printf 'nameserver %s\n' "$resolver" >> "$new"
done

if [ ! -s "$new" ]; then
    echo "apply-ow-dns: OW_DNS=\"${dns}\" contained no resolvers" >&2
    exit 1
fi

cat "$new" > "$resolv_conf"
echo "apply-ow-dns: rewrote ${resolv_conf} from OW_DNS: ${dns}"
