#!/usr/bin/env bash
# Checks that every vx source file maps to a V8 source file at the same path.
#
# The module tree is supposed to mirror V8's, so a file with no counterpart is
# either misnamed or is something V8 keeps somewhere else. Run it against a V8
# checkout:
#
#   tools/check-layout.sh ~/devel/v8
set -u
V8=${1:-$HOME/devel/v8}

if [ ! -d "$V8/src/objects" ]; then
  echo "usage: $0 <path-to-v8-checkout>" >&2
  exit 2
fi

status=0
for file in $(find src -name '*.rs' | grep -Ev '/(mod|lib|main)\.rs$' | sort); do
  rel=${file#src/}
  dir=$(dirname "$rel")
  # vx is snake_case, V8 is kebab-case.
  base=$(basename "$rel" .rs | tr '_' '-')
  found=""
  for extension in h cc tq; do
    candidate="$V8/src/$dir/$base.$extension"
    [ -f "$candidate" ] && found="$found$base.$extension "
  done
  if [ -z "$found" ]; then
    printf '%-40s  NO COUNTERPART in %s/src/%s/\n' "$rel" "$V8" "$dir"
    status=1
  else
    printf '%-40s  <- src/%s/{%s}\n' "$rel" "$dir" "$(echo $found | tr ' ' ',')"
  fi
done

if [ $status -ne 0 ]; then
  echo
  echo "Some files have no V8 counterpart. Either rename them, or record the" >&2
  echo "deviation and add them to the allowlist here." >&2
fi
exit $status
