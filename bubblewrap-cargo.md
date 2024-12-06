# Restrict cargo with bubblewrap

Bubblewrap can be used to prevent rust/cargo modules from accessing your precious files.

```
# shamelessly taken and adapted from https://wiki.archlinux.org/title/Bubblewrap/Examples
# use strace to figure out what cargo wants to access
# strace -ff -yy -s 50 -t -e trace=open,openat,write --no-abbrev /bin/bash bubblewrap.sh cargo remove foo

bwrap_arguments=(
    # no zombies
    --die-with-parent

    # network required for dependencies
    --unshare-all
    --share-net


    # create environment for a properly running shell
    --tmpfs /
    --tmpfs /run
    --dir /tmp
    --dev /dev
    --proc /proc
    --ro-bind /bin /bin
    --ro-bind /sbin /sbin
    --ro-bind /usr/lib /usr/lib
    --ro-bind /lib /lib
    --ro-bind /lib64 /lib64
    --ro-bind /sys /sys
    # --ro-bind /var /var
    # --tmpfs /lib64

    # other etc stuff
    --ro-bind /etc/ssl /etc/ssl
    --ro-bind /etc/ca-certificates /etc/ca-certificates
    --ro-bind /etc/ld.so.cache /etc/ld.so.cache

    # name resolving
    --ro-bind /etc/resolv.conf /etc/resolv.conf
    --ro-bind /etc/hosts /etc/hosts
    --ro-bind /etc/nsswitch.conf /etc/nsswitch.conf
    --ro-bind /etc/gai.conf /etc/gai.conf

    # binaries
    --ro-bind /usr/bin/cargo /usr/bin/cargo
    --ro-bind /usr/bin/cc /usr/bin/cc
    --ro-bind /usr/bin/ld /usr/bin/ld
    --ro-bind /usr/bin/rustc /usr/bin/rustc
    --ro-bind /usr/bin/rustfmt /usr/bin/rustfmt
    --ro-bind /usr/bin/rustdoc /usr/bin/rustdoc

    # # systemd-resolve for dns
    # --ro-bind /run/systemd/resolve /run/systemd/resolve

    # # git is used by npm to init repos, config necessary for email username
    # --ro-bind $HOME/git/config $XDG_CONFIG_HOME/git/config

    # # zsh has to look everywhere cool
    # --ro-bind $XDG_CONFIG_HOME/zsh/.zshrc $XDG_CONFIG_HOME/zsh/.zshrc
    # --ro-bind $XDG_CONFIG_HOME/zsh/.zshenv $XDG_CONFIG_HOME/zsh/.zshenv
    # --ro-bind $HOME/.zshenv $HOME/.zshenv

    # Maven
    # --ro-bind /opt/maven /opt/maven
    --bind $HOME/.cargo $HOME/.cargo

    # # NPM
    # --bind "$XDG_DATA_HOME/npm" "$XDG_DATA_HOME/npm"

    # # cache is needed by many programs like npm, cypress, nvm, maven
    # --bind "$XDG_CACHE_HOME" "$XDG_CACHE_HOME"

    # # x11, needed for cypress
    # --ro-bind "$XAUTHORITY" "$XAUTHORITY"

    # # wayland, might be useful
    # --ro-bind "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY"

    # current dir is assumed to be project dir and full access is allowed
    --bind "$(pwd)" "$(pwd)"
)

# run bwrap with the arguments specified above and with the command provided by the user: zsh, npm install, etc
bwrap "${bwrap_arguments[@]}" "$@"
```