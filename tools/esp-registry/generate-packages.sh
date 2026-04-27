#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)"

VERSION="${WIREMUX_RELEASE_VERSION:-$(tr -d '[:space:]' < "${ROOT_DIR}/VERSION")}"
NAMESPACE="${WIREMUX_ESP_REGISTRY_NAMESPACE:-magicdian}"
REGISTRY_URL="${WIREMUX_ESP_REGISTRY_URL:-}"
REPOSITORY_URL="${WIREMUX_REPOSITORY_URL:-https://github.com/magicdian/wiremux}"
OUTPUT_DIR="${WIREMUX_ESP_REGISTRY_OUTPUT_DIR:-${ROOT_DIR}/dist/esp-registry}"

case "${OUTPUT_DIR}" in
    /*) ;;
    *) OUTPUT_DIR="${ROOT_DIR}/${OUTPUT_DIR}" ;;
esac

case "${OUTPUT_DIR}" in
    "${ROOT_DIR}/dist/esp-registry"|\
    "${ROOT_DIR}/dist/esp-registry/"*) ;;
    *)
        echo "Refusing to write outside ${ROOT_DIR}/dist/esp-registry: ${OUTPUT_DIR}" >&2
        exit 1
        ;;
esac

if ! printf '%s\n' "${VERSION}" | grep -Eq '^[0-9]{4}\.[0-9]{2}\.[0-9]+$'; then
    echo "Version must use YYMM.DD.BuildNumber format, got: ${VERSION}" >&2
    exit 1
fi

CORE_SRC="${ROOT_DIR}/sources/core/c"
ESP_SRC="${ROOT_DIR}/sources/esp32/components/esp-wiremux"
README_TEMPLATE_DIR="${ROOT_DIR}/docs/esp-registry"
CORE_PKG="${OUTPUT_DIR}/wiremux-core"
ESP_PKG="${OUTPUT_DIR}/esp-wiremux"

rm -rf "${OUTPUT_DIR}"
mkdir -p "${CORE_PKG}/include" "${CORE_PKG}/src" "${ESP_PKG}/include" "${ESP_PKG}/src"

cp "${ROOT_DIR}/LICENSE" "${CORE_PKG}/LICENSE"
cp "${ROOT_DIR}/LICENSE" "${ESP_PKG}/LICENSE"
cp "${CORE_SRC}"/include/*.h "${CORE_PKG}/include/"
cp "${CORE_SRC}"/src/*.c "${CORE_PKG}/src/"
cp "${CORE_SRC}"/src/*.h "${CORE_PKG}/src/"
cp "${ESP_SRC}"/include/*.h "${ESP_PKG}/include/"
cp "${ESP_SRC}"/src/*.c "${ESP_PKG}/src/"

render_readme() {
    local template="$1"
    local output="$2"

    sed \
        -e "s|{{version}}|${VERSION}|g" \
        -e "s|{{namespace}}|${NAMESPACE}|g" \
        -e "s|{{repository_url}}|${REPOSITORY_URL}|g" \
        "${template}" > "${output}"
}

render_readme "${README_TEMPLATE_DIR}/wiremux-core/README.md" "${CORE_PKG}/README.md"
render_readme "${README_TEMPLATE_DIR}/esp-wiremux/README.md" "${ESP_PKG}/README.md"
render_readme "${README_TEMPLATE_DIR}/wiremux-core/README_CN.md" "${CORE_PKG}/README_CN.md"
render_readme "${README_TEMPLATE_DIR}/esp-wiremux/README_CN.md" "${ESP_PKG}/README_CN.md"

cat > "${CORE_PKG}/CMakeLists.txt" <<'EOF'
idf_component_register(
    SRCS
        "src/wiremux_batch.c"
        "src/wiremux_compression.c"
        "src/wiremux_frame.c"
        "src/wiremux_envelope.c"
        "src/wiremux_manifest.c"
    INCLUDE_DIRS
        "include"
)
EOF

cat > "${ESP_PKG}/CMakeLists.txt" <<'EOF'
idf_component_register(
    SRCS
        "src/esp_wiremux.c"
        "src/esp_wiremux_console.c"
        "src/esp_wiremux_frame.c"
        "src/esp_wiremux_log.c"
    INCLUDE_DIRS
        "include"
    REQUIRES console esp_driver_usb_serial_jtag esp_system esp_timer log freertos
)
EOF

cat > "${CORE_PKG}/idf_component.yml" <<EOF
version: "${VERSION}"
description: "Portable Wiremux protocol core for ESP-IDF components"
license: "Apache-2.0"
repository: "${REPOSITORY_URL}"
repository_info:
  path: "sources/core/c"
documentation: "${REPOSITORY_URL}/blob/main/docs/esp-registry-release.md"
tags:
  - wiremux
  - protocol
  - serial
  - mux
dependencies:
  idf: ">=5.4"
EOF

cat > "${ESP_PKG}/idf_component.yml" <<EOF
version: "${VERSION}"
description: "Wiremux ESP-IDF adapter component"
license: "Apache-2.0"
repository: "${REPOSITORY_URL}"
repository_info:
  path: "sources/esp32/components/esp-wiremux"
documentation: "${REPOSITORY_URL}/blob/main/docs/esp-registry-release.md"
tags:
  - wiremux
  - esp-idf
  - serial
  - console
dependencies:
  idf: ">=5.4"
  ${NAMESPACE}/wiremux-core:
    version: "${VERSION}"
    require: public
EOF

if [ -n "${REGISTRY_URL}" ]; then
    cat >> "${ESP_PKG}/idf_component.yml" <<EOF
    registry_url: "${REGISTRY_URL}"
EOF
fi

echo "Generated ESP Registry packages in ${OUTPUT_DIR}"
echo "  - wiremux-core ${VERSION}"
echo "  - esp-wiremux ${VERSION} (depends on ${NAMESPACE}/wiremux-core)"
