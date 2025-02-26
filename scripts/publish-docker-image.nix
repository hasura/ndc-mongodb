# This is run via a nix flake package:
#
#   $ nix run .#publish-docker-image <git-ref>
#
# The script is automatically checked with shellcheck, and run with bash using
# a sensible set of options.
{
  # These arguments are passed explicitly
  docker-images
, format ? "oci"
, registry ? { host = "ghcr.io"; repo = "hasura/ndc-mongodb"; }
, target-protocol ? "docker://"

  # These arguments are automatically populated from nixpkgs via `callPackage`
, buildah
, coreutils
, git
, writeShellApplication
}:
writeShellApplication {
  name = "publish-docker-image";
  runtimeInputs = [ coreutils git buildah ];
  text = ''
    # Nix uses the same dollar-braces interpolation syntax as bash so we escape $ as ''$
    if [ -z "''${1+x}" ]; then
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
        local tag="''${BASH_REMATCH[1]}"
        export docker_tags=("$tag")
      elif [[ $input =~ ^refs/heads/(.*)$ ]]; then
        local branch="''${BASH_REMATCH[1]}"
        set_dev_tags "eng-1621"
      else
        export docker_tags=()
      fi
    }

    # We are given separate docker images for each target architecture. Create
    # a list manifest that combines the manifests of each individual image to
    # produce a multi-arch image.
    #
    # The buildah steps are adapted from https://github.com/mirkolenz/flocken
    function publish {
      local manifestName="ndc-mongodb/list"
      local datetimeNow
      datetimeNow="$(TZ=UTC0 date --iso-8601=seconds)"

      if buildah manifest exists "$manifestName"; then
        buildah manifest rm "$manifestName"; 
      fi

      local manifest
      manifest=$(buildah manifest create "$manifestName")  

      for image in ${builtins.toString docker-images}; do
        local manifestOutput
        manifestOutput=$(buildah manifest add "$manifest" "docker-archive:$image")

        local digest
        digest=$(echo "$manifestOutput" | cut "-d " -f2)

        buildah manifest annotate \
          --annotation org.opencontainers.image.created="$datetimeNow" \
          --annotation org.opencontainers.image.revision="$(git rev-parse HEAD)" \
          "$manifest" "$digest"
      done

      echo
      echo "Multi-arch manifests:"
      buildah manifest inspect "$manifest"

      for tag in "''${docker_tags[@]}";
      do
        local image_dest="${target-protocol}${registry.host}/${registry.repo}:$tag"
        echo
        echo "Pushing $image_dest"
        buildah manifest push --all \
          --format ${format} \
          "$manifest" \
          "$image_dest"
      done
    }

    function maybe_publish {
      local input="$1"
      set_docker_tags "$input"
      if [[ ''${#docker_tags[@]} == 0 ]]; then
        echo "The given ref, $input, was not a release tag or a branch - will not publish a docker image"
        exit
      fi

      echo "Will publish docker image with tags: ''${docker_tags[*]}"
      publish
    }

    maybe_publish "$github_ref"
  '';
}
