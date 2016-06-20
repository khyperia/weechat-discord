installdir=$(HOME)/.weechat
testdir=./test_dir

.PHONY: all install install_test test run format
all: src/*
	cargo build --release

install: all | $(installdir)/plugins
	cp target/release/libweecord.so $(installdir)/plugins

install_test: all | $(testdir)/plugins
	cp target/release/libweecord.so $(testdir)/plugins

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
	cargo fmt
	clang-format -style=mozilla -i src/*.c
