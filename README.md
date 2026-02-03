# Yoink

```sh
$ sh <(curl https://yoink.sh) mxcl/brewx
# ^^ installs the latest brewx from its GitHub releases

$ which brewx
~/.local/bin/brewx

$ which yoink
yoink not found  # YOINK NOT YOINKED

$ brewx cowsay we need moar yoink
 ____________________
< we need moar yoink >
 --------------------
        \   ^__^
         \  (oo)\_______
            (__)\       )\/\
                ||----w |
                ||     ||

$ sh <(curl https://yoink.sh) mxcl/yoink
# ^^ yoinking yoink with yoink (like a boss)

$ yoink ls
mxcl/brewx@0.4.2
mxcl/yoink@0.1.0

$ yoink upgrade
# ^^ upgrades everything installed (if possible)

$ yoink rm mxcl/brewx
# ^^ removes brewx
```

### `yoink x`

Why install anything? Just run things.

```sh
$ sh <(curl https://yoink.sh) x denoland/deno eval 'console.log("hi")'
hi

$ which deno
deno not found

$ sh <(curl https://yoink.sh) mxcl/yoink
$ yoink x denoland/deno eval 'console.log("hi")'
hi

$ which deno
deno not found

$ which yoink
~/.local/bin/yoink

$ yoink rm yoink
$ which yoink
yoink not found
```

Go wild.

```sh
$ sh <(curl https://yoink.sh) x mxcl/brewx cowsay hi yoinksters
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
- We provide `yoink x` so you don’t need to install anything to use anything
  even.

## Something Didn’t Work

Report the bug! We’re literally pre 1.0 and open source here!
