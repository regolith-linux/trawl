#/usr/bin/make -f

INSTALL = install
INSTALL_PROGRAM = ${INSTALL} -D -m 0755
INSTALL_DATA = ${INSTALL} -D -m 0644

prefix = ${DESTDIR}/usr
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
datarootdir = $(prefix)/share
libdir = $(exec_prefix)/lib
zshcpl = $(datarootdir)/zsh/site-functions

BIN_D := trawld
BIN_C := trawlcat
BIN_DB := trawldb

all: setup build

distclean: clean

clean:
	-cargo clean
	-rm -r client_api/build

build-arch: build

build-independent: build

binary: build

binary-arch: build

binary-independent: build

build: build-rust build-lib

setup: build-rust
	-make setup-lib

install: 
	$(INSTALL_PROGRAM) "./target/release/$(BIN_C)" "$(bindir)/$(BIN_C)"
	$(INSTALL_PROGRAM) "./target/release/$(BIN_D)" "$(bindir)/$(BIN_D)"
	$(INSTALL_PROGRAM) "./target/release/$(BIN_DB)" "$(bindir)/$(BIN_DB)"
	$(INSTALL_DATA) "./$(BIN_D).service" "$(libdir)/systemd/user/$(BIN_D).service"
	meson install -C client_api/build

gen-service-xml:
	./postbuild.sh


uninstall:
	rm -f "$(bindir)/$(BIN_C)"
	rm -f "$(bindir)/$(BIN_D)"
	rm -f "$(bindir)/$(BIN_DB)"
	rm -f "$(libdir)/systemd/user/$(BIN_D).service"

setup-lib: 
	mkdir -p client_api/build
	touch client_api/build/config_manager.h
	cd client_api && meson build --prefix=$(prefix)
	meson compile -C client_api/build/

test:
	cargo test

code-coverage:
	cargo build --bin $(BIN_D)
	cargo targe/debug/$(BIN_D) &
	cargo tarpaulin -b -- --test-threads 1 
	killall $(BIN_D)

build-lib: build-rust
	cargo build --release
	meson compile -C client_api/build/

build-rust:
	cargo build --release
	./postbuild.sh
	

