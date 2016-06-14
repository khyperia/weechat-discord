installdir=$(HOME)/.weechat/plugins
plugname=weechat-discord.so

.PHONY: all install run

all: $(plugname)

install: $(installdir)/$(plugname)

run: install
	weechat -a

$(installdir)/$(plugname): $(plugname) | $(installdir)
	cp $< $@

$(installdir):
	mkdir $@

$(plugname): *.go *.c
	go build -buildmode=c-shared -o $(plugname)
