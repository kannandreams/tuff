#!/usr/bin/env bash
set -euo pipefail

image="${1:-hello-world}"
message="${2:-hello from a Coral-managed container tool}"
mode="${3:-print}"

case "$image" in
  hello-world)
    cmd=(docker run --rm hello-world)
    ;;
  alpine:3.20)
    cmd=(docker run --rm alpine:3.20 echo "$message")
    ;;
  *)
    echo "unsupported image: $image" >&2
    exit 2
    ;;
esac

printf 'command:'
printf ' %q' "${cmd[@]}"
printf '\n'

if [[ "$mode" == "execute" ]]; then
  "${cmd[@]}"
fi
