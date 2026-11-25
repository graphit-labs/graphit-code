#include "antlr4-runtime.h"
#include "Version.h"

#include <iostream>
#include <string>

static_assert(ANTLRCPP_VERSION_MAJOR == 4);
static_assert(ANTLRCPP_VERSION_MINOR == 13);
static_assert(ANTLRCPP_VERSION_PATCH == 2);
static_assert(ANTLRCPP_VERSION == 413002);

int main() {
  if (antlr4::RuntimeMetaData::VERSION != "4.13.2" ||
      std::string(ANTLRCPP_VERSION_STRING) != "4.13.2") {
    std::cerr << "Unexpected linked ANTLR version: "
              << antlr4::RuntimeMetaData::VERSION << '\n';
    return 1;
  }
  const std::string source = u8"\uFEFFa\U0001F642\u03B2";
  antlr4::ANTLRInputStream input(source);
  if (input.size() != 3 || input.LA(1) != 'a') {
    std::cerr << "SDK UTF-8 conversion or initial BOM handling failed\n";
    return 2;
  }
  input.consume();
  if (input.LA(1) != 0x1F642) {
    std::cerr << "SDK supplementary scalar conversion failed\n";
    return 3;
  }
  input.consume();
  if (input.LA(1) != 0x03B2) {
    std::cerr << "SDK Unicode scalar conversion failed\n";
    return 4;
  }
  input.consume();
  if (input.index() != 3 || input.LA(1) != antlr4::Token::EOF) {
    std::cerr << "SDK EOF/index contract failed\n";
    return 5;
  }
  std::cout << "ANTLR " << antlr4::RuntimeMetaData::VERSION
            << ": linked static runtime, UTF-8/BOM/scalars/EOF passed\n";
}
