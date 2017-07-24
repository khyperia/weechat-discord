installdir=$(HOME)/.weechat
testdir=./test_dir

ifneq ($(wildcard /usr/include/openssl-1.0),)
	export OPENSSL_INCLUDE_DIR=/usr/include/openssl-1.0
else ifneq ($(wildcard /usr/local/opt/openssl/include),)
	export OPENSSL_INCLUDE_DIR=/usr/local/opt/openssl/include
endif
ifneq ($(wildcard /usr/lib/openssl-1.0),)
	export OPENSSL_LIB_DIR=/usr/lib/openssl-1.0
else ifneq ($(wildcard /usr/local/opt/openssl/lib),)
	export OPENSSL_LIB_DIR=/usr/local/opt/openssl/lib
endif

.PHONY: all install install_test test run format clippy
all: src/*
	cargo build --release

install: all | $(installdir)/plugins
	cp target/release/libweecord.* $(installdir)/plugins

install_test: all | $(testdir)/plugins
	cp target/release/libweecord.* $(testdir)/plugins

run: install
	weechat -a

test: install_test
	weechat -a -d $(testdir)

$(installdir):
	mkdir $@

$(installdir)/plugins: | $(installdir)
	mkdir $@

$(testdir):
	mkdir $@

$(testdir)/plugins: | $(testdir)
	mkdir $@

format:
	cargo fmt -- --write-mode=overwrite
	clang-format -style=mozilla -i src/*.c

clippy:
	rustup run nightly cargo rustc --features clippy -- -Z no-trans -Z extra-plugins=clippy
