#!/usr/bin/env bash
#
# To get the skopeo dependency automatically, run with:
#
#     $ nix run .#publish-docker-image <github-ref>
#
set -euo pipefail

IMAGE_PATH=ghcr.io/hasura/ndc-mongodb

if [ -z "${1+x}" ]; then
    echo "Expected argument of the form refs/heads/<branch name> or refs/tags/<tag name>."
    echo "(In a Github workflow the variable github.ref has this format)"
    exit 1
fi

github_ref="$1"

# Assumes that the given ref is a branch name. Sets a tag for a docker image of
# the form:
#
#                dev-main-20230601T1933-bffd555
#                --- ---- ------------- -------
#                ↑   ↑         ↑           ↑
#     prefix "dev"   branch    |        commit hash
#                              |
#                    commit date & time (UTC)
#
# Additionally sets a branch tag assuming this is the latest tag for the given
# branch. The branch tag has the form: dev-main
function set_dev_tags {
    local branch="$1"
    local branch_prefix="dev-$branch"
    local version
    version=$(
        TZ=UTC0 git show \
            --quiet \
            --date='format-local:%Y%m%dT%H%M' \
            --format="$branch_prefix-%cd-%h"
    )
    export docker_tags=("$version" "$branch_prefix")
}

# The Github workflow passes a ref of the form refs/heads/<branch name> or
# refs/tags/<tag name>. This function sets an array of docker image tags based
# on either the given branch or tag name.
#
# If a tag name does not start with a "v" it is assumed to not be a release tag
# so the function sets an empty array.
#
# If the input does look like a release tag, set the tag name as the sole docker
# tag.
#
# If the input is a branch, set docker tags via `set_dev_tags`.
function set_docker_tags {
    local input="$1"
    if [[ $input =~ ^refs/tags/(v.*)$ ]]; then
        local tag="${BASH_REMATCH[1]}"
        export docker_tags=("$tag")
    elif [[ $input =~ ^refs/heads/(.*)$ ]]; then
        local branch="${BASH_REMATCH[1]}"
        set_dev_tags "$branch"
    else
        export docker_tags=()
    fi
}

function maybe_publish {
    local input="$1"
    set_docker_tags "$input"
    if [[ ${#docker_tags[@]} == 0 ]]; then
        echo "The given ref, $input, was not a release tag or a branch - will not publish a docker image"
        exit
    fi

    echo "Will publish docker image with tags: ${docker_tags[*]}"

    nix build .#docker --print-build-logs # writes a tar file to ./result
    ls -lh result
    local image_archive
    image_archive=docker-archive://"$(readlink -f result)"
    skopeo inspect "$image_archive"

    for tag in "${docker_tags[@]}"; do
        echo
        echo "Pushing docker://$IMAGE_PATH:$tag"
        skopeo copy "$image_archive" docker://"$IMAGE_PATH:$tag"
    done
}

maybe_publish "$github_ref"
