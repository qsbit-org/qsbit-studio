PREFIX ?= $(HOME)/.local
DESTDIR ?=
CARGO ?= cargo
PROFILE ?= release
BUILD_DIR = $(if $(filter dev,$(PROFILE)),debug,$(PROFILE))

.PHONY: build install
build:
	$(CARGO) build --locked --profile $(PROFILE) --features exporter

install: build
	install -d "$(DESTDIR)$(PREFIX)/bin" "$(DESTDIR)$(PREFIX)/share/qsbit-studio/presets"
	install -m 755 "target/$(BUILD_DIR)/qsbit-studio" "target/$(BUILD_DIR)/qsbit-blog-export" "$(DESTDIR)$(PREFIX)/bin/"
	install -m 644 presets/*.json "$(DESTDIR)$(PREFIX)/share/qsbit-studio/presets/"
