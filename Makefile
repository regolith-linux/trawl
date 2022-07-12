INSTALL = install
INSTALL_PROGRAM = ${INSTALL} -D -m 0755
INSTALL_DATA = ${INSTALL} -D -m 0644

prefix = /usr
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
datarootdir = $(prefix)/share
libdir = $(exec_prefix)/lib
zshcpl = $(datarootdir)/zsh/site-functions

BIN_D := resmand
BIN_C := rescat
BIN_DB := resdb

all: build

clean:
	cargo clean

install: 
	$(INSTALL_PROGRAM) "./target/release/$(BIN_C)" "$(bindir)/$(BIN_C)"
	$(INSTALL_PROGRAM) "./target/release/$(BIN_D)" "$(bindir)/$(BIN_D)"
	$(INSTALL_PROGRAM) "./target/release/$(BIN_DB)" "$(bindir)/$(BIN_DB)"
	$(INSTALL_DATA) "./$(BIN_D).service" "$(libdir)/systemd/user/$(BIN_D).service"
	meson install -C client_api/build

uninstall:
	rm -f "$(bindir)/$(BIN_C)"
	rm -f "$(bindir)/$(BIN_D)"
	rm -f "$(bindir)/$(BIN_DB)"
	rm -f "$(libdir)/systemd/user/$(BIN_D).service"

build:
	cargo build --release
	meson client_api/build
	meson compile -C client_api/build
test:
	cargo test

code-coverage:
	cargo build --bin resmand
	cargo targe/debug/resmand &
	cargo tarpaulin -b -- --test-threads 1 
	killall resmand

.PHONY: all clean install uninstall build test code-coverage
