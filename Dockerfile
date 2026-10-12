FROM ubuntu:26.04@sha256:f144425ff09be612d6d9ad965196e9cdc23dae1f42110a8a11a3e9a8198759f7 AS runtime-base
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    ca-certificates python3 python3-venv libpython3.14 llvm-21 lld-21 \
    && rm -rf /var/lib/apt/lists/*

FROM runtime-base AS builder
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    curl git build-essential cmake ninja-build python3-dev clang-21 llvm-21-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
ENV PATH="/root/.cargo/bin:/opt/build-tools/bin:/usr/lib/llvm-21/bin:${PATH}" \
    UV_PYTHON_DOWNLOADS=never UV_NO_MANAGED_PYTHON=true
RUN python3 -m venv /opt/build-tools && /opt/build-tools/bin/pip install uv==0.12.7
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup.sh \
    && sh /tmp/rustup.sh -y --profile minimal --default-toolchain none
WORKDIR /src/studio
COPY Cargo.toml Cargo.lock rust-toolchain.toml Makefile LICENSE toolchain.lock.json ./
COPY src ./src
COPY web ./web
COPY presets ./presets
COPY examples ./examples
COPY docker/build-toolchain.sh docker/record-build.py ./docker/
COPY docker/patches ./docker/patches
RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=cache,target=/root/.cache/pip \
    --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/root/.conan2 \
    bash docker/build-toolchain.sh /opt/qsbit /wheels

FROM runtime-base AS runtime
ARG STUDIO_REVISION=unreleased
LABEL org.opencontainers.image.source="https://github.com/qsbit-org/qsbit-studio" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.revision="${STUDIO_REVISION}"
COPY --from=builder /opt/qsbit/bin /opt/qsbit/bin
COPY --from=builder /opt/qsbit/lib/qsbit-sim /opt/qsbit/lib/qsbit-sim
COPY --from=builder /opt/qsbit/share /opt/qsbit/share
RUN --mount=type=bind,from=builder,source=/wheels,target=/wheels \
    python3 -m venv /opt/qsbit/venv \
    && /opt/qsbit/venv/bin/pip install --no-index --find-links=/wheels \
       'qsbit-sim-backends[stim]==0.1.0' qsbit-compiler-tools==0.1.0 \
    && /opt/qsbit/venv/bin/pip check \
    && /opt/qsbit/venv/bin/pip freeze > /opt/qsbit/share/qsbit-studio/python-packages.txt
ENV QSBIT_PYTHON=/opt/qsbit/venv/bin/python \
    PATH="/opt/qsbit/bin:/opt/qsbit/venv/bin:/usr/lib/llvm-21/bin:${PATH}"
COPY docker/check-runtime.py /opt/qsbit/share/qsbit-studio/check-runtime.py
RUN python -c 'import json, os; from pathlib import Path; p = Path("/opt/qsbit/share/qsbit-studio/toolchain.json"); data = json.loads(p.read_text()); data["studio"]["revision"] = os.environ["STUDIO_REVISION"]; p.write_text(json.dumps(data, indent=2))' \
    && dpkg-query -W > /opt/qsbit/share/qsbit-studio/system-packages.txt \
    && useradd --create-home --uid 10001 qsbit \
    && install -d -o qsbit -g qsbit /home/qsbit/runs
USER qsbit
WORKDIR /home/qsbit
RUN python /opt/qsbit/share/qsbit-studio/check-runtime.py
EXPOSE 3000
HEALTHCHECK --interval=15s --timeout=5s --start-period=20s --retries=3 \
    CMD ["python", "-c", "import urllib.request; urllib.request.urlopen('http://127.0.0.1:3000/api/health', timeout=4).read()"]
ENTRYPOINT ["/opt/qsbit/bin/qsbit-studio"]
CMD ["--listen", "0.0.0.0:3000", "--allow-remote", "--work-dir", "/home/qsbit/runs"]
