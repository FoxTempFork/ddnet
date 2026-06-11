# Find glslangValidator
find_program(GLSLANG_VALIDATOR_PROGRAM
  NAMES glslang glslangValidator
)
set(GLSLANG_VALIDATOR_PROGRAM_FOUND TRUE)
if(NOT GLSLANG_VALIDATOR_PROGRAM)
  set(GLSLANG_VALIDATOR_PROGRAM_FOUND FALSE)
  if(TARGET_OS STREQUAL "windows")
    if(${TARGET_CPU_ARCHITECTURE} STREQUAL "x86_64")
      set(GLSLANG_VALIDATOR_PROGRAM "$ENV{VULKAN_SDK}/Bin/glslangValidator.exe")
    else()
      set(GLSLANG_VALIDATOR_PROGRAM "$ENV{VULKAN_SDK}/Bin32/glslangValidator.exe")
    endif()
  endif()

  if(EXISTS ${GLSLANG_VALIDATOR_PROGRAM})
    set(GLSLANG_VALIDATOR_PROGRAM_FOUND TRUE)
  elseif(TARGET_OS STREQUAL "windows" AND ${TARGET_CPU_ARCHITECTURE} STREQUAL "x86_64")
    set(GLSLANG_VALIDATOR_PROGRAM "${PROJECT_SOURCE_DIR}/ddnet-libs/vulkan/windows/lib64/glslangValidator.exe")
    if(EXISTS ${GLSLANG_VALIDATOR_PROGRAM})
      set(GLSLANG_VALIDATOR_PROGRAM_FOUND TRUE)
    endif()
  endif()

  if(NOT GLSLANG_VALIDATOR_PROGRAM_FOUND)
    message(FATAL_ERROR "glslangValidator binary was not found. Install the Vulkan SDK / packages. Alternatively, set VULKAN=OFF to build without Vulkan.")
  endif()
endif()

# Find spirv-opt
find_program(SPIRV_OPTIMIZER_PROGRAM spirv-opt)
set(SPIRV_OPTIMIZER_PROGRAM_FOUND TRUE)
if(NOT SPIRV_OPTIMIZER_PROGRAM)
  set(SPIRV_OPTIMIZER_PROGRAM_FOUND FALSE)
  if(TARGET_OS STREQUAL "windows")
    if (${TARGET_CPU_ARCHITECTURE} STREQUAL "x86_64")
      set(SPIRV_OPTIMIZER_PROGRAM "$ENV{VULKAN_SDK}/Bin/spirv-opt.exe")
    else()
      set(SPIRV_OPTIMIZER_PROGRAM "$ENV{VULKAN_SDK}/Bin32/spirv-opt.exe")
    endif()
  endif()

  if(EXISTS ${SPIRV_OPTIMIZER_PROGRAM})
    set(SPIRV_OPTIMIZER_PROGRAM_FOUND TRUE)
  endif()

  if(NOT SPIRV_OPTIMIZER_PROGRAM_FOUND)
    message(FATAL_ERROR "spirv-opt binary was not found. Install the Vulkan SDK / packages. Alternatively, set VULKAN=OFF to build without Vulkan.")
  endif()
endif()

# Settings
set(TW_VULKAN_VERSION "vulkan100")

# Create output directory
file(MAKE_DIRECTORY "${PROJECT_BINARY_DIR}/data/shader/vulkan/")
set(VULKAN_SHADER_FILE_LIST "")

function(generate_shader_file FILE_ARGS1 FILE_ARGS2 FILE_NAME FILE_OUTPUT_NAME)
  set(OUTPUT_PATH "${PROJECT_BINARY_DIR}/${FILE_OUTPUT_NAME}")
  set(TMP_OUTPUT_PATH "${OUTPUT_PATH}.tmp")

  add_custom_command(
    OUTPUT "${OUTPUT_PATH}"
    COMMAND ${GLSLANG_VALIDATOR_PROGRAM} --client ${TW_VULKAN_VERSION} ${FILE_ARGS1} ${FILE_ARGS2} "${FILE_NAME}" -o "${TMP_OUTPUT_PATH}"
    COMMAND ${SPIRV_OPTIMIZER_PROGRAM} -O "${TMP_OUTPUT_PATH}" -o "${OUTPUT_PATH}"
    COMMAND ${CMAKE_COMMAND} -E remove "${TMP_OUTPUT_PATH}"
    DEPENDS "${FILE_NAME}"
    COMMENT "Compiling Vulkan shader ${FILE_OUTPUT_NAME}"
    VERBATIM
  )

  list(APPEND VULKAN_SHADER_FILE_LIST "${FILE_OUTPUT_NAME}")
  set(VULKAN_SHADER_FILE_LIST ${VULKAN_SHADER_FILE_LIST} PARENT_SCOPE)
endfunction()

# Register all Vulkan shaders to compile
# primitives
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/prim.frag" "data/shader/vulkan/prim.frag.spv")
generate_shader_file("-DTW_TEXTURED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/prim.frag" "data/shader/vulkan/prim_textured.frag.spv")

generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/prim.vert" "data/shader/vulkan/prim.vert.spv")
generate_shader_file("-DTW_TEXTURED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/prim.vert" "data/shader/vulkan/prim_textured.vert.spv")

generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/prim3d.frag" "data/shader/vulkan/prim3d.frag.spv")
generate_shader_file("-DTW_TEXTURED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/prim3d.frag" "data/shader/vulkan/prim3d_textured.frag.spv")

generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/prim3d.vert" "data/shader/vulkan/prim3d.vert.spv")
generate_shader_file("-DTW_TEXTURED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/prim3d.vert" "data/shader/vulkan/prim3d_textured.vert.spv")

# text
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/text.frag" "data/shader/vulkan/text.frag.spv")
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/text.vert" "data/shader/vulkan/text.vert.spv")

# quad container
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/primex.frag" "data/shader/vulkan/primex.frag.spv")
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/primex.vert" "data/shader/vulkan/primex.vert.spv")

generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/primex.frag" "data/shader/vulkan/primex_rotationless.frag.spv")
generate_shader_file("-DTW_ROTATIONLESS" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/primex.vert" "data/shader/vulkan/primex_rotationless.vert.spv")

generate_shader_file("-DTW_TEXTURED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/primex.frag" "data/shader/vulkan/primex_tex.frag.spv")
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/primex.vert" "data/shader/vulkan/primex_tex.vert.spv")

generate_shader_file("-DTW_TEXTURED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/primex.frag" "data/shader/vulkan/primex_tex_rotationless.frag.spv")
generate_shader_file("-DTW_ROTATIONLESS" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/primex.vert" "data/shader/vulkan/primex_tex_rotationless.vert.spv")

generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/spritemulti.frag" "data/shader/vulkan/spritemulti.frag.spv")
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/spritemulti.vert" "data/shader/vulkan/spritemulti.vert.spv")

generate_shader_file("-DTW_PUSH_CONST" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/spritemulti.frag" "data/shader/vulkan/spritemulti_push.frag.spv")
generate_shader_file("-DTW_PUSH_CONST" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/spritemulti.vert" "data/shader/vulkan/spritemulti_push.vert.spv")

# tile layer
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/tile.frag" "data/shader/vulkan/tile.frag.spv")
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/tile.vert" "data/shader/vulkan/tile.vert.spv")

generate_shader_file("-DTW_TILE_TEXTURED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/tile.frag" "data/shader/vulkan/tile_textured.frag.spv")
generate_shader_file("-DTW_TILE_TEXTURED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/tile.vert" "data/shader/vulkan/tile_textured.vert.spv")

generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/tile_border.frag" "data/shader/vulkan/tile_border.frag.spv")
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/tile_border.vert" "data/shader/vulkan/tile_border.vert.spv")

generate_shader_file("" "-DTW_TILE_TEXTURED" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/tile_border.frag" "data/shader/vulkan/tile_border_textured.frag.spv")
generate_shader_file("" "-DTW_TILE_TEXTURED" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/tile_border.vert" "data/shader/vulkan/tile_border_textured.vert.spv")

# quad layer
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/quad.frag" "data/shader/vulkan/quad.frag.spv")
generate_shader_file("" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/quad.vert" "data/shader/vulkan/quad.vert.spv")

generate_shader_file("-DTW_QUAD_GROUPED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/quad.frag" "data/shader/vulkan/quad_grouped.frag.spv")
generate_shader_file("-DTW_QUAD_GROUPED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/quad.vert" "data/shader/vulkan/quad_grouped.vert.spv")

generate_shader_file("-DTW_QUAD_TEXTURED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/quad.frag" "data/shader/vulkan/quad_textured.frag.spv")
generate_shader_file("-DTW_QUAD_TEXTURED" "" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/quad.vert" "data/shader/vulkan/quad_textured.vert.spv")

generate_shader_file("-DTW_QUAD_TEXTURED" "-DTW_QUAD_GROUPED" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/quad.frag" "data/shader/vulkan/quad_grouped_textured.frag.spv")
generate_shader_file("-DTW_QUAD_TEXTURED" "-DTW_QUAD_GROUPED" "${PROJECT_SOURCE_DIR}/data/shader/vulkan/quad.vert" "data/shader/vulkan/quad_grouped_textured.vert.spv")

set(VULKAN_SHADER_FILE_LIST ${VULKAN_SHADER_FILE_LIST} CACHE STRING "Vulkan shader file list" FORCE)

set(VULKAN_SHADER_OUTPUT_PATHS "")
foreach(FILE_OUTPUT_NAME ${VULKAN_SHADER_FILE_LIST})
  list(APPEND VULKAN_SHADER_OUTPUT_PATHS "${PROJECT_BINARY_DIR}/${FILE_OUTPUT_NAME}")
endforeach()

add_custom_target(vulkan_shaders DEPENDS ${VULKAN_SHADER_OUTPUT_PATHS})
