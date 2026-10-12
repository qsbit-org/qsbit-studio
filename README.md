# QSBIT studio

[![CI](https://github.com/qsbit-org/qsbit-studio/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/qsbit-org/qsbit-studio/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/qsbit-org/qsbit-studio)](LICENSE)

QSBIT Studio runs [Bloq](https://github.com/inmzhang/bloq) experiments in a local
web interface: upload a [BLOG file](https://github.com/inmzhang/bloq/blob/main/docs/graphs/blog.md),
run it, and inspect the results.

> [!NOTE]
> A hosted web version of QSBIT Studio is planned.

## Run with Docker

```sh
docker run --rm --init --name qsbit-studio \
  -p 127.0.0.1:3000:3000 ghcr.io/qsbit-org/qsbit-studio:edge
```

Open <http://127.0.0.1:3000>, upload or paste a [BLOG graph](https://bloqec.com/editor/),
and click **Compile & run**. Inspect **Results**, **Replay**, and **Logs**.

`edge` follows tested main builds. Formal releases publish a version tag and `latest`.
For reproducible runs, use an image digest. The image supports Linux AMD64.

For source installation, see [Setup](docs/setup.md). For image builds and publication,
see [Containers](docs/containers.md).

## Limitations

Decoder feedback and dynamic quantum operations are not yet supported.

## Learn more

[Setup details](docs/setup.md) · [Architecture](docs/architecture.md) ·
[API](docs/api.md) · [Testing](docs/testing.md)

## License

[Apache License 2.0](LICENSE).
