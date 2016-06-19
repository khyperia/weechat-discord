installdir=$(HOME)/.weechat/plugins

.PHONY: all install run format
all: src/*
	cargo build --release

install: all | $(installdir)
	cp target/release/libweecord.so $(installdir)

run: install
	weechat -a

$(installdir):
	mkdir $@

format:
	cargo fmt
	clang-format -style=mozilla -i src/*.c
