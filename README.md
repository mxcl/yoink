# Yoink

Download standalone binaries from GitHub releases:

```sh
$ sh <(curl https://yoink.sh) mxcl/brewx
/path/to/brewx
```

By default, stdout prints the downloaded path(s). Stderr also prints
`downloaded: /path/to/brewx` for each binary.

This downloads [`brewx`](https://github.com/mxcl/brewx) into the current
directory but it doesn’t install `yoink` itself:

```sh
$ which yoink
yoink not found
```

If you want `yoink` in your PATH, download it and move it yourself:

```sh
$ sh <(curl https://yoink.sh) mxcl/yoink
/path/to/yoink
$ mv yoink ~/.local/bin/
```

Set `JSON=1` to get machine readable output on stdout:

```sh
$ JSON=1 sh <(curl https://yoink.sh) mxcl/brewx
{
  "repo": "mxcl/brewx",
  "version": "v0.4.2",
  "url": "...",
  "asset": "...",
  "path": "...",
  "paths": ["..."]
}
```

Alternatively, you can just run things:

```sh
$ sh <(curl https://yoink.sh) denoland/deno eval 'console.log("hi")'
hi

$ which deno
deno not found

$ sh <(curl https://yoink.sh) mxcl/yoink
/path/to/yoink
$ ./yoink denoland/deno eval 'console.log("hi")'
hi
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

## Why This and Not All the Other Tools That Seem Identical?

- I tried all the others and they *sucked*.
- We provide a curl one-liner so you don’t even need to install yoink to
  use it. Which is especially nice for READMEs.
- If you pass args after `owner/repo`, yoink runs the binary without
  saving it.

## Something Didn’t Work

Report the bug! We’re literally pre 1.0 and open source here!

## Making Your Repo Yoinkable

1. Upload binaries as tarballs with one folder.
2. Name the binary with platform and architecture in the name, e.g.
   `mytool-linux-x64`.
3. We try to be smart and handle all weird variations so this should be
   sufficient for us to find the right binary for you.
4. If we don't work with your repo, open an issue and we'll do a 3 hour
   turn around for you.
