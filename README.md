# Yoink

Install things that provide standalone binaries on GitHub releases:

```sh
$ sh <(curl https://yoink.sh) mxcl/brewx
installed: ~/.local/bin/brewx
```

This doesn’t install `yoink` itself:

```sh
$ which yoink
yoink not found  # `yoink` NOT YOINKED
```

But it did install `brewx`:

```sh
$ brewx cowsay we need moar yoink
 ____________________
< we need moar yoink >
 --------------------
        \   ^__^
         \  (oo)\_______
            (__)\       )\/\
                ||----w |
                ||     ||
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

Why install anything? You can just run things.

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
