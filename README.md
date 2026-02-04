# Yoink

Install things that provide standalone binaries on GitHub releases:

```sh
$ sh <(curl https://yoink.sh) mxcl/brewx
installed: ~/.local/bin/brewx
```

This installs [`brewx`](https://github.com/mxcl/brewx) but it doesn’t install
`yoink` itself:

```sh
$ which yoink
yoink not found  # `yoink` NOT YOINKED
```

You can install yoink with yoink if you like:

```sh
$ sh <(curl https://yoink.sh) mxcl/yoink
installed: ~/.local/bin/yoink
```

It does package managery stuff too:

```sh
$ yoink ls
mxcl/brewx@0.4.2
mxcl/yoink@0.1.0

$ yoink upgrade
# ^^ upgrades everything installed (if possible)

$ yoink rm mxcl/brewx
# ^^ removes brewx
```

Alternatively, you can just run things:

```sh
$ sh <(curl https://yoink.sh) denoland/deno eval 'console.log("hi")'
hi

$ which deno
deno not found

$ sh <(curl https://yoink.sh) mxcl/yoink
$ yoink denoland/deno eval 'console.log("hi")'
hi

$ which deno
deno not found

$ which yoink
~/.local/bin/yoink

$ yoink rm mxcl/yoink
$ which yoink
yoink not found
```

Go wild.

```sh
$ sh <(curl https://yoink.sh) mxcl/brewx npx cowsay hi yoinksters
 _______________
< hi yoinksters >
---------------
        \   ^__^
         \  (oo)\_______
            (__)\       )\/\
                ||----w |
                ||     ||
```

## Configuring This Thing

`YOINKDIR` - where to install things, defaults to `~/.local/bin`, if you set
it to somewhere that requires `sudo` it will invoke `sudo` for the minimal
`mkdir -p` and atomic `mv` commands required to move the binary into place.

## Why This and Not All the Other Tools That Seem Identical?

- I tried all the others and they *sucked*.
- We provide a curl one-liner so you don’t even need to install yoink to
  use it. Which is especially nice for READMEs.
- If you pass args after `owner/repo`, yoink runs the binary without
  installing it.

## Something Didn’t Work

Report the bug! We’re literally pre 1.0 and open source here!

## Making Your Repo Yoinkable

1. Upload binaries as tarballs with one folder.
2. Name the binary with platform and architecture in the name, e.g. `mytool-linux-x64`.
3. We try to be smart and handle all weird variations so this should be sufficient for us to find the right binary for you.
4. If we don't work with your repo, open an issue and we'll do a 3 hour turn around for you.
