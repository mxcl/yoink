#!/bin/bash

set -exo pipefail

PATH="/opt/homebrew/bin:$PATH"

cd "$(dirname "$0")"

if ! git diff-index --quiet HEAD --; then
  echo "error: dirty working tree" >&2
  exit 1
fi

if [ "$(git rev-parse --abbrev-ref HEAD)" != main ]; then
  echo "error: requires main branch" >&2
  exit 1
fi

if test "$VERBOSE"; then
  set -x
fi

# Prevent AppleDouble (._) files and extended attributes in tar output.
export COPYFILE_DISABLE=1
export COPY_EXTENDED_ATTRIBUTES_DISABLE=1

cargo_toml="$PWD/Cargo.toml"
if [ ! -f "$cargo_toml" ]; then
  echo "error: $cargo_toml not found" >&2
  exit 1
fi

v_new="$(
  python - "$cargo_toml" <<'PY'
import sys
import tomllib

path = sys.argv[1]
with open(path, "rb") as f:
    data = tomllib.load(f)

package = data.get("package", {})
version = None
name = None
if isinstance(package, dict):
    version = package.get("version")
    name = package.get("name")

if not isinstance(version, str) or not version.strip():
    sys.stderr.write("error: version not found in Cargo.toml\n")
    sys.exit(1)

print(version.strip())
PY
)"

bin_name="$(
  python - "$cargo_toml" <<'PY'
import sys
import tomllib

path = sys.argv[1]
with open(path, "rb") as f:
    data = tomllib.load(f)

package = data.get("package", {})
name = None
if isinstance(package, dict):
    name = package.get("name")

if not isinstance(name, str) or not name.strip():
    sys.stderr.write("error: package name not found in Cargo.toml\n")
    sys.exit(1)

print(name.strip())
PY
)"

if [ "$(npx --yes -- semver "$v_new")" != "$v_new" ]; then
  echo "error: Cargo.toml version $v_new is not valid semver" >&2
  exit 1
fi

case $1 in
"")
  clobber=false
  ;;
clobber)
  clobber=true
  ;;
*)
  echo "usage $0 [clobber]" >&2
  exit 1
  ;;
esac

# ensure we have the latest version tags
git fetch origin -pft

# ensure github tags the right release
git push origin main

if versions="$(git tag | grep '^v[0-9]\+\.[0-9]\+\.[0-9]\+')"; then
  v_latest="$(npx --yes -- semver --include-prerelease $versions | tail -n1)"
fi

if [ -n "$v_latest" ]; then
  v_max="$(npx --yes -- semver --include-prerelease "$v_new" "$v_latest" | tail -n1)"
  if [ "$v_max" != "$v_new" ]; then
    echo "error: Cargo.toml version $v_new is older than latest tag v$v_latest" >&2
    exit 1
  fi
fi

if git tag -l "v$v_new" | grep -q .; then
  if [ "$clobber" = false ]; then
    echo "error: v$v_new already exists (use clobber to reupload)" >&2
    exit 1
  fi
fi

if [ "$clobber" = true ]; then
  true
elif ! gh release view v$v_new >/dev/null 2>&1; then
  gum confirm "prepare draft release for $v_new?" || exit 1

  gh release create \
    v$v_new \
    --draft=true \
    --generate-notes \
    $([ -n "$v_latest" ] && [ "$v_latest" != "$v_new" ] && echo "--notes-start-tag=v$v_latest") \
    --title=v$v_new
else
  gum format "> existing $v_new release found, using that"
  echo  # spacer
fi

targets=(
  aarch64-apple-darwin
  x86_64-apple-darwin
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-gnu
)

toolchain="$(rustup show active-toolchain | awk '{print $1}')"
host_uname_s="$(uname -s)"

strip_binary() {
  local bin_path="$1"
  local target="$2"

  if command -v llvm-strip >/dev/null 2>&1; then
    llvm-strip "$bin_path"
    return
  fi

  case "$target" in
  *-apple-darwin)
    if command -v strip >/dev/null 2>&1 && [ "$host_uname_s" = "Darwin" ]; then
      strip -x "$bin_path"
      return
    fi
    ;;
  *-unknown-linux-gnu)
    if command -v strip >/dev/null 2>&1 && [ "$host_uname_s" = "Linux" ]; then
      strip --strip-unneeded "$bin_path"
      return
    fi
    ;;
  esac

  echo "note: skip strip for $target (no compatible strip found)"
}

for target in "${targets[@]}"; do
  if ! rustup target list | grep -Eq "^${target}([[:space:]]|$)"; then
    echo "skip: $target not supported by $toolchain"
    continue
  fi

  case "$target" in
  aarch64-apple-darwin)
    uname_s=Darwin
    uname_m=arm64
    ;;
  x86_64-apple-darwin)
    uname_s=Darwin
    uname_m=x86_64
    ;;
  x86_64-unknown-linux-gnu)
    uname_s=Linux
    uname_m=x86_64
    ;;
  aarch64-unknown-linux-gnu)
    uname_s=Linux
    uname_m=aarch64
    ;;
  *)
    echo "error: unsupported target $target" >&2
    exit 1
    ;;
  esac

  artifact="$bin_name-$v_new-$uname_s-$uname_m.tar.gz"
  bin_file="$bin_name"
  if [[ "$target" == *-pc-windows-* ]]; then
    bin_file="$bin_file.exe"
  fi

  rustup target add "$target"

  if [ "$target" = "aarch64-apple-darwin" ] && [ "$(uname -s)" = "Darwin" ]; then
    cargo build --release --target "$target"
  elif [ "$target" = "x86_64-apple-darwin" ] && [ "$(uname -s)" = "Darwin" ]; then
    cargo build --release --target "$target"
  else
    cargo zigbuild --release --target "$target"
  fi

  bin_path="target/$target/release/$bin_file"
  strip_binary "$bin_path" "$target" || true

  rm -f "$artifact"
  tar -C "target/$target/release" -czf "$artifact" "$bin_file"

  gh release upload --clobber v$v_new "$artifact"
done

gh release view v$v_new

if [ "$clobber" = false ]; then
  gum confirm "draft prepared, release $v_new?" || exit 1

  gh release edit \
    v$v_new \
    --verify-tag \
    --latest \
    --draft=false
fi

cloudfront_function_name="yoink-sh-root"
cloudfront_function_file="$(mktemp -t yoink-sh-function.XXXXXX.js)"
cloudfront_origin_id="yoink-sh-site"
site_bucket="${YOINK_SITE_BUCKET:-yoink-sh-site}"
site_region="${YOINK_SITE_REGION:-$(aws configure get region || true)}"
cloudfront_distribution_id="${YOINK_CLOUDFRONT_DISTRIBUTION_ID:-}"
cloudfront_oac_name="${YOINK_SITE_OAC_NAME:-yoink-sh-site}"

if [ -z "$site_region" ]; then
  site_region="us-east-1"
fi

if [ -z "$cloudfront_distribution_id" ]; then
  cloudfront_distribution_id="$(
    aws cloudfront list-distributions \
      --query "DistributionList.Items[?Aliases.Items && contains(Aliases.Items, 'yoink.sh')].Id | [0]" \
      --output text
  )"
fi

if [ -z "$cloudfront_distribution_id" ] || [ "$cloudfront_distribution_id" = "None" ]; then
  echo "error: cloudfront distribution for yoink.sh not found" >&2
  exit 1
fi

if ! aws s3api head-bucket --bucket "$site_bucket" >/dev/null 2>&1; then
  if [ "$site_region" = "us-east-1" ]; then
    aws s3api create-bucket --bucket "$site_bucket" --region "$site_region"
  else
    aws s3api create-bucket \
      --bucket "$site_bucket" \
      --region "$site_region" \
      --create-bucket-configuration "LocationConstraint=$site_region"
  fi
fi

if [ ! -f "$PWD/index.html" ]; then
  echo "error: index.html not found (site sync)" >&2
  exit 1
fi

if [ ! -f "$PWD/preview.webp" ]; then
  echo "error: preview.webp not found (site sync)" >&2
  exit 1
fi

cloudfront_oac_id="$(
  aws cloudfront list-origin-access-controls \
    --query "OriginAccessControlList.Items[?Name=='$cloudfront_oac_name'].Id | [0]" \
    --output text
)"

if [ -z "$cloudfront_oac_id" ] || [ "$cloudfront_oac_id" = "None" ]; then
  cloudfront_oac_id="$(
    aws cloudfront create-origin-access-control \
      --origin-access-control-config \
        "Name=$cloudfront_oac_name,Description=yoink.sh site origin access,SigningProtocol=sigv4,SigningBehavior=always,OriginAccessControlOriginType=s3" \
      --query 'OriginAccessControl.Id' \
      --output text
  )"
fi

if [ "$site_region" = "us-east-1" ]; then
  site_origin_domain="${site_bucket}.s3.amazonaws.com"
else
  site_origin_domain="${site_bucket}.s3.${site_region}.amazonaws.com"
fi

cloudfront_distribution_config_file="$(mktemp -t yoink-sh-dist.XXXXXX.json)"
cloudfront_distribution_etag="$(
  aws cloudfront get-distribution-config \
    --id "$cloudfront_distribution_id" \
    --query 'ETag' \
    --output text
)"

aws cloudfront get-distribution-config \
  --id "$cloudfront_distribution_id" \
  --query 'DistributionConfig' \
  --output json >"$cloudfront_distribution_config_file"

python - "$cloudfront_distribution_config_file" \
  "$cloudfront_origin_id" \
  "$site_origin_domain" \
  "$cloudfront_oac_id" <<'PY'
import json
import sys

path = sys.argv[1]
origin_id = sys.argv[2]
origin_domain = sys.argv[3]
oac_id = sys.argv[4]

with open(path, "r", encoding="utf-8") as handle:
    config = json.load(handle)

origins = config.setdefault("Origins", {"Items": [], "Quantity": 0})
items = origins.get("Items") or []
origin = next((item for item in items if item.get("Id") == origin_id), None)
if origin is None:
    origin = {"Id": origin_id}
    items.append(origin)

origin["DomainName"] = origin_domain
origin.setdefault("OriginPath", "")
origin["CustomHeaders"] = {"Quantity": 0}
origin["ConnectionAttempts"] = origin.get("ConnectionAttempts", 3)
origin["ConnectionTimeout"] = origin.get("ConnectionTimeout", 10)
origin["S3OriginConfig"] = {"OriginAccessIdentity": ""}
origin["OriginAccessControlId"] = oac_id
origin.pop("CustomOriginConfig", None)

origins["Items"] = items
origins["Quantity"] = len(items)

default_behavior = config.get("DefaultCacheBehavior")
if isinstance(default_behavior, dict):
    default_behavior["TargetOriginId"] = origin_id

config["DefaultRootObject"] = "index.html"

with open(path, "w", encoding="utf-8") as handle:
    json.dump(config, handle, ensure_ascii=False, indent=2)
PY

aws cloudfront update-distribution \
  --id "$cloudfront_distribution_id" \
  --if-match "$cloudfront_distribution_etag" \
  --distribution-config "file://$cloudfront_distribution_config_file"

account_id="$(aws sts get-caller-identity --query Account --output text)"
bucket_policy="$(
  python - "$site_bucket" "$account_id" "$cloudfront_distribution_id" <<'PY'
import json
import sys

bucket = sys.argv[1]
account_id = sys.argv[2]
distribution_id = sys.argv[3]

policy = {
    "Version": "2012-10-17",
    "Statement": [
        {
            "Sid": "AllowCloudFrontRead",
            "Effect": "Allow",
            "Principal": {"Service": "cloudfront.amazonaws.com"},
            "Action": "s3:GetObject",
            "Resource": [
                f"arn:aws:s3:::{bucket}/index.html",
                f"arn:aws:s3:::{bucket}/preview.webp",
            ],
            "Condition": {
                "StringEquals": {
                    "AWS:SourceArn": (
                        f"arn:aws:cloudfront::{account_id}:distribution/"
                        f"{distribution_id}"
                    )
                }
            },
        }
    ],
}

print(json.dumps(policy))
PY
)"

aws s3api put-bucket-policy \
  --bucket "$site_bucket" \
  --policy "$bucket_policy"

aws s3 sync \
  --only-show-errors \
  --delete \
  --cache-control "public, max-age=300" \
  --exclude "*" \
  --include "index.html" \
  --include "preview.webp" \
  "$PWD/" "s3://$site_bucket/"

aws cloudfront create-invalidation \
  --distribution-id "$cloudfront_distribution_id" \
  --paths "/*"

llms_body_lines="$(
  python - <<'PY'
import json
from pathlib import Path

text = Path("llms.txt").read_text()
lines = text.splitlines()
if text.endswith("\n"):
    lines.append("")
for line in lines:
    print(f"      {json.dumps(line)},")
PY
)"

cat <<'EOF' >"$cloudfront_function_file"
function handler(event) {
  var request = event.request;
  var headers = request.headers || {};
  var userAgentHeader = headers['user-agent'];
  var userAgent = userAgentHeader ? userAgentHeader.value.toLowerCase() : '';
  var isCli = userAgent.indexOf('curl') !== -1 || userAgent.indexOf('wget') !== -1;
  var isIndexGet = request.method === 'GET' &&
    (request.uri === '/' || request.uri === '/index.html');
  var isPreviewGet = request.method === 'GET' &&
    request.uri === '/preview.webp';
  var isLlmsGet = request.method === 'GET' && request.uri === '/llms.txt';

  if (isLlmsGet) {
    var body = [
EOF

printf '%s\n' "$llms_body_lines" >>"$cloudfront_function_file"

cat <<'EOF' >>"$cloudfront_function_file"
    ].join("\n");

    return {
      statusCode: 200,
      statusDescription: 'OK',
      headers: {
        'content-type': { value: 'text/plain; charset=utf-8' },
        'cache-control': { value: 'no-store' }
      },
      body: body,
      bodyEncoding: 'text'
    };
  }

  if (request.method === 'GET' && request.uri === '/' && isCli) {
    var body = [
      "tmp=$(mktemp -d)",
      "trap 'rm -rf \"$tmp\"' EXIT",
      "curl -LSsf \"https://github.com/mxcl/yoink/releases/download/v@VERSION@/yoink-@VERSION@-$(uname -s)-$(uname -m).tar.gz\" | tar -xz -C \"$tmp\"",
      "\"$tmp/yoink\" \"$@\"",
      ""
    ].join("\n");

    return {
      statusCode: 200,
      statusDescription: 'OK',
      headers: {
        'content-type': { value: 'text/plain; charset=utf-8' },
        'cache-control': { value: 'no-store' }
      },
      body: body,
      bodyEncoding: 'text'
    };
  }

  if (isIndexGet) {
    request.uri = '/index.html';
    return request;
  }

  if (isPreviewGet) {
    return request;
  }

  return {
    statusCode: 302,
    statusDescription: 'Found',
    headers: {
      location: { value: 'https://github.com/mxcl/yoink' },
      'cache-control': { value: 'no-store' }
    }
  };
}
EOF

sed -i '' "s/@VERSION@/$v_new/g" "$cloudfront_function_file"

cloudfront_function_etag="$(
  aws cloudfront describe-function \
    --name "$cloudfront_function_name" \
    --stage DEVELOPMENT \
    --query 'ETag' \
    --output text
)"

cloudfront_function_etag="$(
  aws cloudfront update-function \
    --name "$cloudfront_function_name" \
    --if-match "$cloudfront_function_etag" \
    --function-config Comment="yoink.sh root handler",Runtime="cloudfront-js-2.0" \
    --function-code "fileb://$cloudfront_function_file" \
    --query 'ETag' \
    --output text
)"

aws cloudfront publish-function \
  --name "$cloudfront_function_name" \
  --if-match "$cloudfront_function_etag"

gh release view v$v_new --web
