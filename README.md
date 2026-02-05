# Yoink
<a href="https://coveralls.io/github/mxcl/yoink?branch=main">
  <img alt="Coverage Status"
       src="https://coveralls.io/repos/github/mxcl/yoink/badge.svg?branch=main">
</a>

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

## GitHub Actions

Use yoink as a composite action (Linux and macOS runners).

```yaml
- uses: mxcl/yoink@v0.4.0
  id: yoink
  with:
    repo: cli/cli

- name: Show version
  env:
    YOINK_DIR: ${{ steps.yoink.outputs.download_dir }}
    YOINK_EXE: >-
      ${{ fromJSON(steps.yoink.outputs.executables)[0] }}
  run: |
    "${YOINK_DIR}/${YOINK_EXE}" --version
```

To run a downloaded binary directly:

```yaml
- uses: mxcl/yoink@v0.4.0
  with:
    repo: denoland/deno
    args: |
      eval
      console.log("hi from yoink action")
```

Outputs (download mode):

- `repo`
- `tag`
- `url`
- `executables` (JSON array)
- `download_dir`
- `yoink_version`

If `args` is set, the action runs the downloaded binary and skips the
download outputs. `args` is newline-separated; each line becomes one
argument.

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
> With yoink and tools like `pkgx` you and your agents can run tools without
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

### Other Stuff

```sh
$ sh <(curl https://yoink.sh) -j cli/cli
{
  "repo": "cli/cli",
  "tag": "v2.86.0",
  "url": "https://github.com/cli/cli/releases/download/v2.86.0/gh_2.86.0_macOS_arm64.zip",
  "executables": ["gh"]
}

$ ./gh --version
gh version 2.86.0
```

```sh
$ sh <(curl https://yoink.sh) -C $(mktemp -d) astral-sh/uv | xargs sudo install -m 755 -D /usr/local/bin

$ ls /usr/local/bin/uv*
/usr/local/bin/uv
/usr/local/bin/uvx
# ^^ installed both executables from the release asset
```

```sh
# “headers only” useful for doing an “outdated” check
$ sh <(curl https://yoink.sh) -jI direnv/direnv
{
  "repo": "direnv/direnv",
  "tag": "v2.37.1",
  "url": "https://github.com/direnv/direnv/releases/download/v2.37.1/direnv.darwin-arm64"
}

$ ls ./direnv
ls: ./direnv: No such file or directory
```

#### Platforms

We have almost no platform specific code and will work on every platform that
Rust supports.

> Adding support to ./publish-release.sh for your platform is very welcome.
> If you do so we will backfill the releases table.


## Why This and Not All the Other Tools That Seem Identical?

- I tried all the others and didn’t like them.
- We provide a curl one-liner so you don’t even need to install yoink to
  use it. Which is especially nice for READMEs.
- If you pass args after `owner/repo`, yoink runs the binary without
  saving it.

## Vibecoding a Package Manager on Top of Yoink

Do a combination of this:

```sh
$ sh <(curl https://yoink.sh) -C $(mktemp -d) astral-sh/uv | xargs sudo install -m 755 -D /usr/local/bin
```

And “headers only” checks to do outdated.

```sh
$ sh <(curl https://yoink.sh) -jI astral-sh/uv
{
  "repo": "astral-sh/uv",
  "tag": "v0.4.0",
}
```

Then vibe code a script to check the installed `uv --version` against the
latest version that yoink can give you.

> I did this: https://github.com/mxcl/bootstrap


## Something Didn’t Work

Report the bug! We’re literally pre 1.0 and open source here!

## Ensuring Your Repo is Yoinkable

1. Upload binaries as tarballs with one folder.
2. Name the binary with platform and architecture in the name, e.g.
   `mytool-linux-x64`.
3. We try to be smart and handle all weird variations so this should be
   sufficient for us to find the right binary for you.
4. If we don't work with your repo, open an issue and we'll do a 3 hour
   turn around for you.
