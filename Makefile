#/usr/bin/make -f

INSTALL_PROGRAM = install -D -m 0755
INSTALL_DATA = install -D -m 0644

prefix = ${DESTDIR}/usr
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
datarootdir = $(prefix)/share
libdir = $(exec_prefix)/lib
zshcpl = $(datarootdir)/zsh/site-functions

BIN_D := trawld
BIN_C := trawlcat
BIN_DB := trawldb

MESON = meson


all: build

distclean: clean

clean:
	-cargo clean

build-arch: build

build-independent: build

binary: build

binary-arch: build

binary-independent: build

build: 
	cargo build --release

install: 
	sudo $(INSTALL_PROGRAM) "./target/release/$(BIN_C)" "$(bindir)/$(BIN_C)"
	sudo $(INSTALL_PROGRAM) "./target/release/$(BIN_D)" "$(bindir)/$(BIN_D)"
	sudo $(INSTALL_PROGRAM) "./target/release/$(BIN_DB)" "$(bindir)/$(BIN_DB)"
	sudo $(INSTALL_DATA) "./$(BIN_D).service" "$(libdir)/systemd/user/$(BIN_D).service"


uninstall:
	rm -f "$(bindir)/$(BIN_C)"
	rm -f "$(bindir)/$(BIN_D)"
	rm -f "$(bindir)/$(BIN_DB)"
	rm -f "$(libdir)/systemd/user/$(BIN_D).service"

run-test:
	cargo test -- --test-threads=1

code-coverage:
	cargo build --bin $(BIN_D)
	cargo targe/debug/$(BIN_D) &
	cargo tarpaulin -b -- --test-threads 1 
	killall $(BIN_D)
