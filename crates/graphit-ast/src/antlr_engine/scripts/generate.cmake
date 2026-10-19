cmake_minimum_required(VERSION 3.15)

if(NOT DEFINED ANTLR_TOOL_JAR OR NOT EXISTS "${ANTLR_TOOL_JAR}")
  message(FATAL_ERROR "ANTLR_TOOL_JAR must name the official 4.13.2 JAR")
endif()
get_filename_component(ANTLR_TOOL_JAR "${ANTLR_TOOL_JAR}" ABSOLUTE)
if(NOT DEFINED OUTPUT_DIRECTORY OR OUTPUT_DIRECTORY STREQUAL "")
  message(FATAL_ERROR "OUTPUT_DIRECTORY is required")
endif()
get_filename_component(OUTPUT_DIRECTORY "${OUTPUT_DIRECTORY}" ABSOLUTE)
if(EXISTS "${OUTPUT_DIRECTORY}" OR EXISTS "${OUTPUT_DIRECTORY}.staging")
  message(FATAL_ERROR "Generation requires a new output directory: ${OUTPUT_DIRECTORY}")
endif()
find_package(Java 11 REQUIRED COMPONENTS Runtime)
execute_process(
  COMMAND "${Java_JAVA_EXECUTABLE}" -cp "${ANTLR_TOOL_JAR}" org.antlr.v4.Tool
  RESULT_VARIABLE version_result OUTPUT_VARIABLE banner ERROR_VARIABLE version_error)
if(NOT version_result EQUAL 0 OR NOT banner MATCHES "Version 4\\.13\\.2([^0-9]|$)")
  message(FATAL_ERROR "Expected official ANTLR Tool 4.13.2 (${version_result}):\n${banner}\n${version_error}")
endif()
get_filename_component(grammar_root "${CMAKE_CURRENT_LIST_DIR}/../grammars" ABSOLUTE)
set(staging "${OUTPUT_DIRECTORY}.staging")
file(MAKE_DIRECTORY "${staging}")

function(generate family grammar)
  set(output "${staging}/${family}")
  file(MAKE_DIRECTORY "${output}")
  execute_process(
    COMMAND "${Java_JAVA_EXECUTABLE}" -cp "${ANTLR_TOOL_JAR}" org.antlr.v4.Tool
      -Dlanguage=Cpp -encoding UTF-8 -Xexact-output-dir -no-listener -no-visitor
      -package "graphit::antlr::${family}" -lib "${output}" -o "${output}"
      "${grammar}.g4"
    WORKING_DIRECTORY "${grammar_root}/${family}"
    RESULT_VARIABLE result OUTPUT_VARIABLE tool_output ERROR_VARIABLE tool_error)
  # Preserve real diagnostics, including nonfatal grammar warnings.
  file(APPEND "${staging}/tool-diagnostics.log"
    "${family}/${grammar}: exit ${result}\n${tool_output}${tool_error}\n")
  if(NOT tool_output STREQUAL "")
    message("${tool_output}")
  endif()
  if(NOT tool_error STREQUAL "")
    message("${tool_error}")
  endif()
  if(NOT result EQUAL 0)
    file(REMOVE_RECURSE "${staging}")
    message(FATAL_ERROR "ANTLR failed for ${family}/${grammar} (${result}); output was not published")
  endif()
endfunction()

generate(plsql PlSqlLexer)
generate(plsql PlSqlParser)
generate(postgresql PostgreSQLLexer)
generate(postgresql PostgreSQLParser)
generate(db2 Db2Lexer)
generate(db2 Db2Parser)
generate(tsql TSqlLexer)
generate(tsql TSqlParser)
generate(cobol85 Cobol85)
generate(cobol85 Cobol85Preprocessor)
file(COPY "${grammar_root}/GRAMMAR_NOTICES.md" DESTINATION "${staging}")
file(RENAME "${staging}" "${OUTPUT_DIRECTORY}")
message(STATUS "Generated all five families and COBOL preprocessor: ${OUTPUT_DIRECTORY}")
