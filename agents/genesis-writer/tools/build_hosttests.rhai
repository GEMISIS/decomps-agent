// @tool build_hosttests
// @description Build and run the host test suite exactly as the user's machine will: every compiler line of `make test` is re-run with -Wall -Wextra -Werror -Wimplicit-function-declaration (the sandbox gcc is lenient, the user's clang is not). Prints PASS/FAIL per test binary with the first errors and a HOSTTESTS summary line. A suite that fails here is a defect even if plain `make test` passes: fix missing includes, implicit declarations and C89 violations in the test files.
// @requires shell
shell(`test -x observe/host_tests_strict.sh || echo "observe/host_tests_strict.sh missing"; observe/host_tests_strict.sh . 2>&1 | head -n 80`)
