Weechat Discord
===============

Currently a work in progress. I deeply apologize to anyone who may stumble upon this repository in hopes of finding an elusive weechat-discord bridge before it is finished, or worse, stumble upon it and find it abandoned.

### Building

The makefile should give enough information for build commands. Here's the essentials:

    cd weechat-discord # or wherever you cloned it
    go build -buildmode=c-shared -o weechat-discord.so

This will produce a shared object called `weechat-discord.so`. Place it in your weechat plugins directory, which is probably located at `~/.weechat/plugins` (may need to be created)

The Makefile has a tiny bit of automation that helps with development:

    make # (same as make all) just runs that `go build` command, produces weechat-discord.so
    make install # builds and copies the .so to ~/.weechat/plugins, creating the dir if required
    make run # installs and runs `weechat -a` (-a means "don't autoconnect to servers")

IMPORTANT: weechat is **Very Not Okay** with weechat-discord.so being modified while weechat is running, even if unloaded. If weechat-discord.so is modified, restart weechat before attempting to reload, otherwise you will get a SIGSEGV and hard crash.

### Using

Right now, due to it being a work in progress, things aren't fully fleshed out yet.

    /set plugins.var.weechat-discord.email your.email@example.com
    /set plugins.var.weechat-discord.password yourpassword
    /discord

Note that it will eventually probably be something like `/discord connect`, as well as other helper subcommands of `/discord` (like setting those email/password variables), but again, this is a work in progress.
