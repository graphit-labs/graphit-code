#pragma once

// Load the runtime's system headers before removing platform math macros
// that collide with generated COBOL and DB2 token names.
#include "antlr4-runtime.h"
#include <cmath>
#undef OVERFLOW
#undef INFINITY
#undef NAN
#undef SNAN
