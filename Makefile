#/usr/bin/make -f

INSTALL = install
INSTALL_PROGRAM = ${INSTALL} -D -m 0755
INSTALL_DATA = ${INSTALL} -D -m 0644
MESON = python -m meson

prefix = ${DESTDIR}/usr
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
datarootdir = $(prefix)/share
libdir = $(exec_prefix)/lib
zshcpl = $(datarootdir)/zsh/site-functions

BIN_D := trawld
BIN_C := trawlcat
BIN_DB := trawldb


all: build

distclean: clean

clean:
	-cargo clean
	-rm -r client_api/build

build-arch: build

build-independent: build

binary: build

binary-arch: build

binary-independent: build

build: 
	make build-rust
	-make setup
	make build-lib


setup: gen-service-xml setup-lib

install: 
	$(INSTALL_PROGRAM) "./target/release/$(BIN_C)" "$(bindir)/$(BIN_C)"
	$(INSTALL_PROGRAM) "./target/release/$(BIN_D)" "$(bindir)/$(BIN_D)"
	$(INSTALL_PROGRAM) "./target/release/$(BIN_DB)" "$(bindir)/$(BIN_DB)"
	$(INSTALL_DATA) "./$(BIN_D).service" "$(libdir)/systemd/user/$(BIN_D).service"
	$(MESON) install -C client_api

gen-service-xml:
	./postbuild.sh

uninstall:
	rm -f "$(bindir)/$(BIN_C)"
	rm -f "$(bindir)/$(BIN_D)"
	rm -f "$(bindir)/$(BIN_DB)"
	rm -f "$(libdir)/systemd/user/$(BIN_D).service"

setup-lib: 
	pip install meson ninja
	export PATH=~/.local/bin:$PATH
	mkdir -p client_api/build
	touch client_api/build/config_manager.h
	cd client_api && $(MESON) build --prefix=$(prefix)
	$(MESON) compile -C client_api

test:
	cargo test

code-coverage:
	cargo build --bin $(BIN_D)
	cargo targe/debug/$(BIN_D) &
	cargo tarpaulin -b -- --test-threads 1 
	killall $(BIN_D)

build-lib: 
	$(MESON) compile -C client_api

build-rust:
	cargo build --release

