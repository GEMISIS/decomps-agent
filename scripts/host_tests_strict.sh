#!/usr/bin/env bash
# Build and run the host test suite the way the USER's machine will: every compiler line of `make test`
# is re-run with -Wall -Wextra -Werror -Wimplicit-function-declaration added, whatever the Makefile says.
# The sandbox's gcc is lenient and clang on macOS is not, so a suite that only passes leniently is a
# defect the writer, Remedy, Forge and the real-asset smoke test must all see the same way.
#   observe/host_tests_strict.sh [workdir]   -> per-binary PASS/FAIL lines + "HOSTTESTS: N pass, M fail"
cd "${1:-.}" || exit 2
STRICT="-Wall -Wextra -Werror -Wimplicit-function-declaration"
mkdir -p build
lines=$(make -n test 2>/dev/null | grep -E '^[[:space:]]*(gcc|cc|clang)[[:space:]]')
if [ -z "$lines" ]; then echo "HOSTTESTS: no compiler lines in 'make -n test' (add a test target that compiles test/*.c with gcc)"; echo "HOSTTESTS: 0 pass, 1 fail"; exit 1; fi
pass=0; fail=0
while IFS= read -r line; do
  [ -n "$line" ] || continue
  cmd=$(printf '%s' "$line" | sed -E "s/^([[:space:]]*)(gcc|cc|clang)[[:space:]]/\1\2 $STRICT /")
  name=$(printf '%s' "$line" | grep -oE 'test/[A-Za-z0-9_]+\.c' | head -1); name="${name:-$line}"
  if out=$(bash -e -o pipefail -c "$cmd" 2>&1); then pass=$((pass+1)); echo "PASS  $name"
  else fail=$((fail+1)); echo "FAIL  $name"; printf '%s\n' "$out" | grep -iE 'error|assert|fail' | head -4 | sed 's/^/      /'; fi
done <<< "$lines"
for t in test/test_*.c; do [ -f "$t" ] || continue
  printf '%s\n' "$lines" | grep -q "$t" || { fail=$((fail+1)); echo "FAIL  $t is not built by 'make test' (every test/test_*.c must be wired into the test target)"; }
done
echo "HOSTTESTS: $pass pass, $fail fail"
[ "$fail" -eq 0 ]
