INSTALL = install
INSTALL_PROGRAM = ${INSTALL} -D -m 0755
INSTALL_DATA = ${INSTALL} -D -m 0644

prefix = /usr
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
datarootdir = $(prefix)/share
libdir = $(exec_prefix)/lib
zshcpl = $(datarootdir)/zsh/site-functions

BIN_D := trawld
BIN_C := trawlcat
BIN_DB := trawldb

all: build

clean:
	cargo clean

install: 
	sudo $(INSTALL_PROGRAM) "./target/release/$(BIN_C)" "$(bindir)/$(BIN_C)"
	sudo $(INSTALL_PROGRAM) "./target/release/$(BIN_D)" "$(bindir)/$(BIN_D)"
	sudo $(INSTALL_PROGRAM) "./target/release/$(BIN_DB)" "$(bindir)/$(BIN_DB)"
	sudo $(INSTALL_DATA) "./$(BIN_D).service" "$(libdir)/systemd/user/$(BIN_D).service"
	meson install -C client_api/build

uninstall:
	rm -f "$(bindir)/$(BIN_C)"
	rm -f "$(bindir)/$(BIN_D)"
	rm -f "$(bindir)/$(BIN_DB)"
	rm -f "$(libdir)/systemd/user/$(BIN_D).service"

setup:
	mkdir -p client_api/build
	touch client_api/build/config_manager.h
	cd client_api && meson setup build
	meson compile -C client_api/build/

build:
	cargo build --release
	echo "Ensure you have the latest build of $(BIN_D) running"
	./postbuild.sh
	meson compile -C client_api/build/
test:
	cargo test

code-coverage:
	cargo build --bin $(BIN_D)
	cargo targe/debug/$(BIN_D) &
	cargo tarpaulin -b -- --test-threads 1 
	killall $(BIN_D)

.PHONY: all clean install uninstall setup build test code-coverage
