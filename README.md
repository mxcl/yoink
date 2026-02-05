# Yoink

## Downloading Standalone Binaries

```sh
$ sh <(curl https://yoink.sh) mxcl/brewx
./brewx

$ ./brewx --version
brewx 0.4.2
```

> [!TIP]
>
> `yoink` is *not installed* but if you like you can yoink it:
>
> ```sh
> $ sh <(curl https://yoink.sh) -C ~/.local/bin mxcl/yoink
> ```

## Executing Standalone Binaries

Often you don’t want to keep the thing even.

```sh
$ sh <(curl https://yoink.sh) denoland/deno eval 'console.log("hi")'
hi

$ ls ./deno
ls: ./deno: No such file or directory
```

> [!TIP]
>
> Installing things can have unexpected effects on systems.
> Via yoink and tools like `pkgx` you and your agents can run tools without
> installing them.
>
> ```sh
> $ sh <(curl https://yoink.sh) pkgxdev/pkgx npx cowsay hi yoinksters
>  _______________
> < hi yoinksters >
> ---------------
>         \   ^__^
>          \  (oo)\_______
>             (__)\       )\/\
>                 ||----w |
>                 ||     ||
> ```

## Other Stuff

```sh
$ sh <(curl https://yoink.sh) -j mxcl/brewx
{
  "repo": "mxcl/brewx",
  "tag": "v0.4.2",
  "url": "https://github.com/mxcl/brewx/releases/download/v0.4.2/brewx-macos-arm64.tar.gz",
  "asset": "brewx-macos-arm64.tar.gz",
  "paths": ["/cwd/brewx"]
}
```

```sh
$ sh <(curl https://yoink.sh) -C $(mktemp -d) mxcl/brewx | xargs sudo install -m 755 -D /usr/local/bin
# ^^ invokes sudo but only when atomically moving the binary into place
```

```sh
# “headers only” useful for doing an “outdated” check
$ sh <(curl https://yoink.sh) -jI direnv/direnv
{
  "repo": "direnv/direnv",
  "tag": "v0.4.2",
  …
}

$ ls ./brewx
ls: ./brewx: No such file or directory
```



## Why This and Not All the Other Tools That Seem Identical?

- I tried all the others and didn’t like them.
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
