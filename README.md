# Yoink

- Package manager(ish) for binary downloads from GitHub releases
- Package executor for one-off runs of binaries from GitHub releases

## Usage

```sh
$ sh <(curl https://yoink.sh) mxcl/brewx
# ^^ -s ensures sh reads from stdin even when args are present
# ^^ installs the latest brewx from its GitHub releases
# DOES NOT INSTALL YOINK

$ which brewx
~/.local/bin/brewx

$ sh <(curl https://yoink.sh) mxcl/yoink
# ^^ installs yoink itself (if you like)

$ yoink ls
mxcl/brewx@0.4.2
mxcl/yoink@0.1.0

$ yoink upgrade
# ^^ upgrades everything installed

$ yoink rm mxcl/brewx
# ^^ removes brewx
```

### `yoink x`

Why install anything? Just run things.

```sh
$ sh <(curl https://yoink.sh/x) denoland/deno run 'console.log("hi")'
hi

$ which deno
deno not found

$ sh <(curl https://yoink.sh) mxcl/yoink
$ yoink x denoland/deno run 'console.log("hi")'
hi

$ which deno
deno not found
```

## Configuring This Thing

`YOINKDIR` - where to install things, defaults to `~/.local/bin`, if you set
it to somewhere that requires `sudo` it will invoke `sudo` for the minimal
`mkdir -p` and atomic `mv` commands required to move the binary into place.

## Why This and Not All the Other Tools That Seem Identical?

I tried all the others and thought they *sucked*.

Also we provide a curl one-liner so you don’t even need to install yoink to
use it. Which is especially nice for READMEs.

## Something Didn’t Work

Report the bug! We’re literally pre 1.0 and open source here!
