# Yoink

Package manager (ish) for binary downloads from GitHub releases.

## Usage

```sh
$ curl https://yoink.sh | sh -- mxcl/brewx
# ^^ installs the latest brewx from its GitHub releases
# DOES NOT INSTALL YOINK

$ curl https://yoink.sh | sh -- mxcl/yoink
# ^^ installs yoink itself
# NOTE you don’t need to install yoink, just use the curl command above

$ yoink ls
mxcl/brewx@0.4.2
mxcl/yoink@0.1.0

$ yoink upgrade
# ^^ upgrades everything installed

$ yoink rm mxcl/brewx
# ^^ removes brewx
```

## Configuring This Thing

`YOINKDIR` - where to install things, defaults to `~/.local/bin`, if you set
it to somewhere that requires `sudo` it will invoke `sudo` for the minimal
atomic `mv` command required to move the binary into place.

## Why This and Not All the Other Tools That Seem Identical?

I tried all the others and thought they *sucked*.

Also we provide a curl one-liner so you don’t even need to install yoink to
use it. Which is especially nice for READMEs.
