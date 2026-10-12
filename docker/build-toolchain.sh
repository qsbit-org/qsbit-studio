#!/usr/bin/env bash
set -euo pipefail
prefix="${1:?installation prefix}"
wheels="${2:?wheel directory}"
studio="$(pwd)"
jobs="${BUILD_JOBS:-2}"
mkdir -p "$wheels" "$prefix/share/qsbit-studio"
python3 - <<'PY'
import json, subprocess
from pathlib import Path
lock = json.loads(Path("toolchain.lock.json").read_text())
for name in ("simulator", "compiler"):
    path = Path("/src") / name
    spec = lock[name]
    subprocess.run(["git", "init", str(path)], check=True)
    subprocess.run(["git", "-C", str(path), "fetch", "--no-tags", "--depth=1", spec["repository"], spec["revision"]], check=True)
    subprocess.run(["git", "-C", str(path), "checkout", "--detach", "FETCH_HEAD"], check=True)
    assert subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD"], text=True).strip() == spec["revision"]
PY
cd /src/simulator
uv sync --python /usr/bin/python3 --no-managed-python --frozen --group build --extra stim
.venv/bin/conan config install conan/config
.venv/bin/conan install . -pr:a=conan/profiles/clang -s:a build_type=Release \
    --build=missing -c tools.build:jobs="$jobs" --lockfile=conan.lock
python_include="$(/usr/bin/python3 -c 'import sysconfig; print(sysconfig.get_path("include"))')"
python_library="$(/usr/bin/python3 -c 'import sysconfig; print(sysconfig.get_config_var("LIBDIR") + "/" + sysconfig.get_config_var("LDLIBRARY"))')"
cmake --preset clang-ninja-release -DBUILD_TESTING=OFF -DQSBIT_BUILD_EXAMPLES=OFF \
    -DQSBIT_PYTHON_BACKENDS=ON -DPython3_EXECUTABLE="$PWD/.venv/bin/python" \
    -DPython3_INCLUDE_DIR="$python_include" -DPython3_LIBRARY="$python_library"
cmake --build --preset clang-ninja-release --parallel "$jobs"
cmake --install build-clang-release --prefix "$prefix"
uv export --frozen --extra stim --no-dev --no-emit-project --format requirements-txt > /tmp/runtime-requirements.txt
/opt/build-tools/bin/pip download --require-hashes --only-binary=:all: \
    -r /tmp/runtime-requirements.txt --dest "$wheels"
uv pip install --python .venv/bin/python setuptools==80.9.0 wheel==0.45.1
uv build --wheel --no-build-isolation --python .venv/bin/python --out-dir "$wheels"
cd /src/compiler
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_COMPILER=clang++-21 \
    -DBUILD_TESTING=OFF -DCMAKE_PREFIX_PATH="$prefix"
cmake --build build --parallel "$jobs"
cmake --install build --prefix "$prefix"
uv build --wheel --no-build-isolation --python /src/simulator/.venv/bin/python --out-dir "$wheels"
cd "$studio"
make install PREFIX="$prefix"
install -d "$prefix/share/qsbit-studio/examples"
install -m 644 examples/*.blog "$prefix/share/qsbit-studio/examples/"
cargo metadata --locked --features exporter --format-version=1 > /tmp/cargo-metadata.json
python3 docker/record-build.py "$prefix" "${STUDIO_REVISION:-unreleased}"
