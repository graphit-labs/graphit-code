# Official ANTLR C++ runtime 4.13.2

Source: https://www.antlr.org/download/antlr4-cpp-runtime-4.13.2-source.zip

The runtime sources, headers, CMake configuration, VERSION, README and license
are retained. Demo, upstream test suite, generated build directories, downloaded
GoogleTest dependencies and prebuilt binaries are excluded. Tests and demos are
disabled during the private static build; no vendor dependency is downloaded.

`runtime/src/Version.h` defines `ANTLRCPP_VERSION_MAJOR/MINOR/PATCH`, but the
official archive's combined numeric and string macros reference undefined
`ANTLR4CPP_VERSION_MAJOR/MINOR/PATCH`. Those six references are corrected to the
defined names. No other runtime implementation is changed. The copyright and
BSD license remain intact. The linked runtime must report `4.13.2`.

The official shared DFA is used. `ANTLR4_USE_THREAD_LOCAL_CACHE` is not enabled.
