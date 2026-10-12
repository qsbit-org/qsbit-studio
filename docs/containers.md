# Containers

## Build and test

Docker Buildx builds the versions in `toolchain.lock.json`:

```sh
docker build --build-arg STUDIO_REVISION="$(git rev-parse HEAD)" -t qsbit-studio:local .
uv sync --frozen --group integration --group browser
uv run --no-sync playwright install --with-deps chromium
uv run --no-sync python tests/container_test.py --image qsbit-studio:local
```

Run the test on a local Linux Docker Engine. It checks loopback port publishing,
then runs the final image on an internal Docker network using its bridge address. It verifies the
installed tools, Stim results, upload, text input, example selection, replay,
downloads, invalid inputs, cancellation, timeouts, and shutdown cleanup.

## Runtime

The image runs as UID 10001, with Python at `/opt/qsbit/venv`.
Studio listens on `0.0.0.0:3000` inside the container. Publish the port on
`127.0.0.1` to restrict host access. The service has no authentication.

The image contains LLVM 21 tools and runtime libraries. Source trees, native
build tools, browser test dependencies, and build caches stay outside the final
image. Startup and simulation do not download dependencies.

Component revisions, package inventories, and native dependency licenses are
under `/opt/qsbit/share/qsbit-studio`. Python package licenses remain in their
installed distributions; system package licenses are under `/usr/share/doc`.

## Publication

The Container workflow builds and tests Linux AMD64 images on pull requests.
Successful main builds publish `edge` and a unique commit-and-run tag.
A published, non-prerelease GitHub release with a semantic version tag publishes
that version and `latest`. Manual runs validate without publishing.

The publication job loads the exact tested image and checks its image ID before
pushing. It then logs out and verifies anonymous retrieval by digest.
On first publication, an organization package administrator must set the GHCR
package visibility to **Public**. Anonymous verification fails until this is done.

Update component revisions through a pull request. ARM64 images require their
own build and final-image tests before publication.
