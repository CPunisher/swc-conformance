#!/usr/bin/env bash

set -euo pipefail

project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
common_git_dir="$(git -C "$project_root" rev-parse --path-format=absolute --git-common-dir)"
fixtures_root="${1:-$project_root/fixtures}"
repository_root="${2:-$common_git_dir/swc-conformance/fixture-cache/repositories}"

prepare_fixture() {
    local repo="$1" name="$2" ref="$3"
    local path="$fixtures_root/$name"
    local repository_path="$repository_root/$name"
    local pin_ref="refs/fixture-pins/$ref"

    git init --quiet --bare --object-format=sha1 "$repository_path"
    if ! git --git-dir="$repository_path" rev-parse --verify --quiet "${pin_ref}^{commit}" > /dev/null; then
        git --git-dir="$repository_path" fetch --quiet --no-tags --depth 1 \
            "https://github.com/$repo.git" "${ref}:${pin_ref}"
    fi

    rm -rf -- "$path"
    mkdir -p "$path"
    git --git-dir="$repository_path" archive "$pin_ref" | tar -xf - -C "$path"
}

prepare_fixture "tc39/test262" "test262" "d53ebc8b1931d9f1541e43d59650776e4b766248"
prepare_fixture "microsoft/TypeScript" "typescript" "5237fd46890956e4bb8177a1a05dcd69e4166560"
prepare_fixture "babel/babel" "babel" "93e39e4d0b657efe7f569f9d25acbff96ddb89dc"
prepare_fixture "oxc-project/estree-conformance" "estree-conformance" "eb050aeacb0d0588f84829da0a5cbf0c63676637"
prepare_fixture "swc-project/swc" "swc" "4768dbe091a6706a009d2e9c08950cacba107d55"

rm -rf -- "$fixtures_root/test262-legacy"
cp -R -- "$project_root/fixtures-test262-legacy" "$fixtures_root/test262-legacy"
