#!/usr/bin/env bash
#
# Updates the runa packages for the AUR, Homebrew and Scoop to a release.
#
# Usage: packaging/release.sh <version>
#
# Run it in a clean clone of runa on main. With the version that is already
# packaged, it only verifies the packages and changes nothing. After a failed
# run, run it again with the same version to finish what is left.

set -euo pipefail

REPO="runa-fm/runa"
WORKFLOW="$REPO/.github/workflows/releases.yml"
AUR_URL="https://aur.archlinux.org/runa.git"
AUR_PUSH_URL="ssh://aur@aur.archlinux.org/runa.git"
ARCH_BOX="runa-arch"

PKGBUILD="packaging/arch/PKGBUILD"
SRCINFO="packaging/arch/.SRCINFO"
FORMULA="Formula/runa.rb"
MANIFEST="bucket/runa.json"

WIN_X64="runa-windows-x86_64.zip"
WIN_ARM64="runa-windows-aarch64.zip"
BREW_FILES=(
    runa-macos-x86_64.tar.gz
    runa-macos-aarch64.tar.gz
    runa-linux-x86_64-gnu.tar.gz
    runa-linux-aarch64-gnu.tar.gz
)
PACKAGE_FILES=("${BREW_FILES[@]}" "$WIN_X64" "$WIN_ARM64")
ASSETS=(SHA256SUMS.txt "${PACKAGE_FILES[@]}")

VERSION_RE='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
SUMS_RE='^([0-9a-f]{64}) [ *](runa-[a-z0-9_.-]+)$'
BREW_VERSION_RE='^(  version ")[0-9.]+"$'
BREW_URL_RE='^ +url "https://github\.com/runa-fm/runa/releases/download/'
BREW_URL_RE+='v#\{version\}/(runa-[a-z0-9_-]+\.tar\.gz)"$'
BREW_SHA_RE='^( +sha256 ")([0-9a-f]{64})"$'

VERSION=""
TAG=""
TMP=""
AUR_DIR=""
BREW_DIR=""
SCOOP_DIR=""
COMMIT=""
ARCHIVE_SUM=""
PACKAGED_VERSION=""
PKGREL=""
HINT=""
declare -A SUM

fail() {
    printf '\nError: %b\n' "$*" >&2
    exit 1
}

step() {
    printf '\n==> %s\n' "$*"
}

hash_of() {
    local line
    line=$(sha256sum -- "$1")
    echo "${line%% *}"
}

changed_files() {
    git -C "$1" diff --name-only | tr '\n' ' '
}

# Removes the temporary directory and prints the hint for a failed commit or push.
on_exit() {
    local status=$?
    rm -rf -- "$TMP"
    if [[ $status -ne 0 && -n $HINT ]]; then
        printf '\n%s\n' "$HINT" >&2
    fi
}

# Checks that the tools are the system ones and that nothing redirects git or gh.
check_environment() {
    # shellcheck disable=SC2046
    unset $(compgen -e -X '!GIT_*') GH_HOST GH_REPO
    export LC_ALL=C GH_PROMPT_DISABLED=1

    [[ $EUID -ne 0 ]] || fail "do not run this as root"

    local tool path
    for tool in git gh jq sha256sum gzip cmp ssh toolbox; do
        path=$(command -v "$tool") || fail "$tool not found"
        [[ $(realpath -- "$path") == /usr/bin/* ]] || fail "$tool is $path, not a system binary"
    done
    toolbox run -c "$ARCH_BOX" sh -c 'command -v updpkgsums && command -v makepkg' >/dev/null ||
        fail "the $ARCH_BOX toolbox needs base-devel and pacman-contrib"
    gh auth status >/dev/null 2>&1 || fail "gh is not logged in, run: gh auth login"
}

# Checks that this is a runa clone from runa-fm, clean and equal to origin/main.
check_runa() {
    local url_re='^(https://github\.com/|git@github\.com:)runa-fm/runa(\.git)?$'

    [[ $(git rev-parse --show-toplevel) == "$PWD" ]] || fail "$PWD is not a runa clone"

    local fetch_url push_url
    fetch_url=$(git remote get-url origin)
    push_url=$(git remote get-url --push origin)
    [[ $fetch_url =~ $url_re ]] || fail "runa fetches from $fetch_url"
    [[ $push_url =~ $url_re ]] || fail "runa pushes to $push_url"

    git fetch -q origin
    [[ -z $(git status --porcelain) ]] || fail "runa has uncommitted changes"
    [[ $(git branch --show-current) == main ]] || fail "runa is not on main"
    [[ $(git rev-parse HEAD) == "$(git rev-parse origin/main)" ]] ||
        fail "runa is not equal to origin/main"
}

# Clones a runa-fm repository on main. It pushes over the same protocol as runa.
clone_repo() {
    local name=$1 dir=$2
    local runa_path="runa-fm/runa"
    git clone -q --no-tags -- "https://github.com/runa-fm/$name.git" "$dir"

    local push_url
    push_url=$(git remote get-url --push origin)
    git -C "$dir" remote set-url --push origin "${push_url/"$runa_path"/runa-fm/$name}"
    [[ $(git -C "$dir" branch --show-current) == main ]] || fail "$name is not on main"
}

# Checks line endings, the version order and the PKGBUILD source.
check_package_files() {
    local file
    for file in "$PKGBUILD" "$SRCINFO" "$BREW_DIR/$FORMULA" "$SCOOP_DIR/$MANIFEST"; do
        ! grep -q $'\r' -- "$file" || fail "$file has CRLF line endings"
    done

    local newest
    PACKAGED_VERSION=$(sed -n 's/^pkgver=//p' "$PKGBUILD")
    newest=$(printf '%s\n' "$PACKAGED_VERSION" "$VERSION" | sort -V | tail -n 1)
    [[ $newest == "$VERSION" ]] || fail "$VERSION is older than the packaged $PACKAGED_VERSION"

    # updpkgsums runs the PKGBUILD, so it must still download runa's tag archive
    local line
    # shellcheck disable=SC2016
    for line in \
        'pkgname=runa' \
        'url="https://github.com/runa-fm/runa"' \
        'source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")'; do
        grep -qFx -- "$line" "$PKGBUILD" || fail "PKGBUILD is missing the line: $line"
    done

    local key
    for key in pkgver pkgrel sha256sums; do
        [[ $(grep -c "^$key=" "$PKGBUILD") -eq 1 ]] || fail "PKGBUILD needs exactly one $key= line"
    done
}

# Clones the AUR package and checks that it equals packaging/arch at a commit on main.
# Changes to packaging/arch after that commit ship with the release.
check_aur() {
    git clone -q --no-tags -- "$AUR_URL" "$AUR_DIR"
    git -C "$AUR_DIR" remote set-url --push origin "$AUR_PUSH_URL"

    [[ $(git -C "$AUR_DIR" branch --show-current) == master ]] || fail "the AUR is not on master"
    [[ $(git -C "$AUR_DIR" ls-files | tr '\n' ' ') == ".SRCINFO PKGBUILD " ]] ||
        fail "the AUR has unexpected files"

    local commit
    while IFS= read -r commit; do
        if cmp -s -- "$AUR_DIR/PKGBUILD" <(git show "$commit:$PKGBUILD" 2>/dev/null) &&
            cmp -s -- "$AUR_DIR/.SRCINFO" <(git show "$commit:$SRCINFO" 2>/dev/null); then
            echo "The AUR equals packaging/arch at $(git --no-pager log --oneline -1 "$commit")"
            return
        fi
    done < <(git rev-list HEAD -- packaging/arch)
    fail "the AUR does not equal packaging/arch at any commit on main"
}

# Checks that the release is published and that its tag is a commit on main.
check_release() {
    local state
    state=$(gh release view "$TAG" -R "$REPO" --json tagName,isDraft,isPrerelease \
        --jq '"\(.tagName) \(.isDraft) \(.isPrerelease)"') || fail "release $TAG not found"
    [[ $state == "$TAG false false" ]] || fail "release $TAG is a draft or a prerelease"

    COMMIT=$(gh api "repos/$REPO/commits/refs/tags/$TAG" --jq .sha)
    [[ $COMMIT =~ ^[0-9a-f]{40}$ ]] || fail "could not resolve $TAG to a commit"

    local relation
    relation=$(gh api "repos/$REPO/compare/$COMMIT...main" --jq .status)
    [[ $relation == ahead || $relation == identical ]] || fail "$TAG is not on runa's main"
    echo "$TAG is commit $COMMIT on main"
}

# Downloads the assets, verifies their attestations and reads SHA256SUMS.txt.
verify_assets() {
    local dir=$TMP/assets asset
    mkdir -- "$dir"

    for asset in "${ASSETS[@]}"; do
        gh release download "$TAG" -R "$REPO" -D "$dir" -p "$asset"
        [[ -f $dir/$asset ]] || fail "$asset is missing from release $TAG"
        gh attestation verify "$dir/$asset" -R "$REPO" \
            --signer-workflow "$WORKFLOW" \
            --source-ref "refs/tags/$TAG" \
            --source-digest "$COMMIT" \
            --deny-self-hosted-runners >/dev/null || fail "attestation failed for $asset"
    done

    local line name
    while IFS= read -r line; do
        [[ $line =~ $SUMS_RE ]] || fail "unexpected line in SHA256SUMS.txt: $line"
        name=${BASH_REMATCH[2]}
        [[ -z ${SUM[$name]:-} ]] || fail "SHA256SUMS.txt lists $name twice"
        SUM[$name]=${BASH_REMATCH[1]}
    done <"$dir/SHA256SUMS.txt"

    for asset in "${PACKAGE_FILES[@]}"; do
        [[ -n ${SUM[$asset]:-} ]] || fail "$asset is not in SHA256SUMS.txt"
        [[ $(hash_of "$dir/$asset") == "${SUM[$asset]}" ]] ||
            fail "$asset does not match SHA256SUMS.txt"
    done
    echo "All ${#ASSETS[@]} assets are attested and match SHA256SUMS.txt"
}

# Updates the PKGBUILD with the official tools and checks the downloaded archive.
# A new version starts at pkgrel 1, the packaged version keeps its pkgrel.
update_aur() {
    if [[ $VERSION != "$PACKAGED_VERSION" ]]; then
        sed -i -e "s/^pkgver=.*/pkgver=$VERSION/" -e "s/^pkgrel=.*/pkgrel=1/" "$PKGBUILD"
    fi
    PKGREL=$(sed -n 's/^pkgrel=//p' "$PKGBUILD")
    [[ $PKGREL =~ ^[1-9][0-9]*$ ]] || fail "PKGBUILD has an invalid pkgrel: $PKGREL"

    mkdir -- "$TMP/src"
    (
        cd packaging/arch
        toolbox run -c "$ARCH_BOX" env SRCDEST="$TMP/src" updpkgsums
        toolbox run -c "$ARCH_BOX" makepkg --printsrcinfo >.SRCINFO
    )

    # GitHub writes the commit into the archive header
    local archive=$TMP/src/runa-$VERSION.tar.gz
    [[ -f $archive ]] || fail "updpkgsums did not download $archive"
    gzip -dc -- "$archive" >"$TMP/source.tar"
    [[ $(git get-tar-commit-id <"$TMP/source.tar") == "$COMMIT" ]] ||
        fail "the source archive is not commit $COMMIT"
    ARCHIVE_SUM=$(hash_of "$archive")
}

# Writes the version and the hashes. Each sha256 line belongs to the url line above it.
update_homebrew() {
    local line file=""
    while IFS= read -r line; do
        if [[ $line =~ $BREW_VERSION_RE ]]; then
            line="${BASH_REMATCH[1]}$VERSION\""
        elif [[ $line =~ $BREW_URL_RE ]]; then
            file=${BASH_REMATCH[1]}
        elif [[ -n $file && $line =~ $BREW_SHA_RE ]]; then
            line="${BASH_REMATCH[1]}${SUM[$file]}\""
            file=""
        fi
        printf '%s\n' "$line"
    done <"$BREW_DIR/$FORMULA" >"$TMP/runa.rb"
    cat -- "$TMP/runa.rb" >"$BREW_DIR/$FORMULA"
}

# Writes the version, the urls and the hashes.
update_scoop() {
    jq --indent 4 \
        --arg version "$VERSION" \
        --arg base "https://github.com/$REPO/releases/download/$TAG" \
        --arg x64 "${SUM[$WIN_X64]}" \
        --arg arm64 "${SUM[$WIN_ARM64]}" \
        '.version = $version
        | .architecture."64bit".url = "\($base)/runa-windows-x86_64.zip"
        | .architecture."64bit".hash = $x64
        | .architecture.arm64.url = "\($base)/runa-windows-aarch64.zip"
        | .architecture.arm64.hash = $arm64' \
        "$SCOOP_DIR/$MANIFEST" >"$TMP/runa.json"
    cat -- "$TMP/runa.json" >"$SCOOP_DIR/$MANIFEST"
}

verify_aur() {
    local line
    for line in "pkgver=$VERSION" "pkgrel=$PKGREL" "sha256sums=('$ARCHIVE_SUM')"; do
        grep -qFx -- "$line" "$PKGBUILD" || fail "PKGBUILD is missing the line: $line"
    done
    for line in \
        "pkgver = $VERSION" \
        "pkgrel = $PKGREL" \
        "sha256sums = $ARCHIVE_SUM" \
        "source = runa-$VERSION.tar.gz::https://github.com/$REPO/archive/$TAG.tar.gz"; do
        grep -qFx -- $'\t'"$line" "$SRCINFO" || fail ".SRCINFO is missing the line: $line"
    done
    echo "PKGBUILD and .SRCINFO match the source archive"
}

verify_homebrew() {
    grep -qFx -- "  version \"$VERSION\"" "$BREW_DIR/$FORMULA" ||
        fail "runa.rb is not at version $VERSION"

    local -A seen
    local line file="" hash
    while IFS= read -r line; do
        if [[ $line =~ $BREW_URL_RE ]]; then
            file=${BASH_REMATCH[1]}
        elif [[ $line =~ $BREW_SHA_RE ]]; then
            hash=${BASH_REMATCH[2]}
            [[ -n $file ]] || fail "runa.rb has a sha256 line without a url line above it"
            [[ -z ${seen[$file]:-} ]] || fail "runa.rb lists $file twice"
            [[ $hash == "${SUM[$file]:-}" ]] || fail "runa.rb has the wrong hash for $file"
            seen[$file]=1
            file=""
        fi
    done <"$BREW_DIR/$FORMULA"

    for file in "${BREW_FILES[@]}"; do
        [[ -n ${seen[$file]:-} ]] || fail "runa.rb has no hash for $file"
    done
    [[ ${#seen[@]} -eq ${#BREW_FILES[@]} ]] || fail "runa.rb has unexpected downloads"
    echo "The version and all ${#BREW_FILES[@]} hashes match the release"
}

verify_scoop() {
    local base="https://github.com/$REPO/releases/download/$TAG"
    local expected actual
    expected=$(printf '%s\n' \
        "$VERSION" \
        "$base/$WIN_X64" "${SUM[$WIN_X64]}" "runa-windows-x86_64" \
        "$base/$WIN_ARM64" "${SUM[$WIN_ARM64]}" "runa-windows-aarch64")
    actual=$(jq -r '.version, (.architecture."64bit", .architecture.arm64
        | .url, .hash, .extract_dir)' "$SCOOP_DIR/$MANIFEST")
    [[ $actual == "$expected" ]] || fail "runa.json does not match the release:\n$actual"
    echo "The version, both urls and both hashes match the release"
}

# Fails unless a repository changed nothing or exactly the expected files.
check_files() {
    local dir=$1 name=$2 expected=$3
    [[ -z $(git -C "$dir" ls-files --others --exclude-standard) ]] ||
        fail "untracked files in $name"

    local changed
    changed=$(changed_files "$dir")
    [[ -z $changed || $changed == "$expected" ]] ||
        fail "unexpected changes in $name: $changed"
}

# Fails unless the line changes in a file all match a pattern.
check_lines() {
    local dir=$1 file=$2 allowed=$3
    local bad
    bad=$(git -C "$dir" diff -U0 -- "$file" | grep -E '^[+-]' | grep -vE '^(\+\+\+|---) ' |
        grep -vE "$allowed" || true)
    [[ -z $bad ]] || fail "unexpected line changes in $file:\n$bad"
}

# Checks that only the version, url and hash lines of the package files changed.
# The AUR copy equals packaging/arch, so its own diff needs no line check.
check_changes() {
    check_files . runa "$SRCINFO $PKGBUILD "
    check_files "$AUR_DIR" AUR ".SRCINFO PKGBUILD "
    check_files "$BREW_DIR" homebrew-tap "$FORMULA "
    check_files "$SCOOP_DIR" scoop-bucket "$MANIFEST "

    check_lines . "$PKGBUILD" '^[+-](pkgver|pkgrel|sha256sums)='
    check_lines . "$SRCINFO" $'^[+-]\t(pkgver|pkgrel|source|sha256sums) = '
    check_lines "$BREW_DIR" "$FORMULA" '^[+-] +(version|sha256) "'
    check_lines "$SCOOP_DIR" "$MANIFEST" '^[+-] +"(version|url|hash)": '
    echo "Only version, url and hash lines changed"
}

# Commits files in a repository that has changes and checks that the commit is signed.
commit() {
    local dir=$1 message=$2
    shift 2
    [[ -n $(changed_files "$dir") ]] || return 0
    git -C "$dir" add -- "$@"
    git -C "$dir" commit -q -m "$message" -- "$@"
    [[ $(git -C "$dir" cat-file commit HEAD) == *$'\ngpgsig '* ]] ||
        fail "the commit in $dir is not signed"
    git -C "$dir" --no-pager log --oneline -1
}

commit_all() {
    commit . "chore(packaging): update aur package to $VERSION" "$PKGBUILD" "$SRCINFO"
    commit "$AUR_DIR" "Update to $TAG" PKGBUILD .SRCINFO
    commit "$BREW_DIR" "runa: Update to version $VERSION" "$FORMULA"
    commit "$SCOOP_DIR" "runa: Update to version $VERSION" "$MANIFEST"
}

# Pushes a repository that is ahead of its remote and checks the remote branch.
push() {
    local dir=$1 name=$2 branch=$3 ssh=$4
    [[ $(git -C "$dir" rev-parse HEAD) != "$(git -C "$dir" rev-parse "origin/$branch")" ]] ||
        return 0
    git -C "$dir" -c core.sshCommand="$ssh" push -q origin "$branch"

    local remote
    remote=$(git -C "$dir" ls-remote origin "refs/heads/$branch")
    [[ ${remote%%[[:space:]]*} == "$(git -C "$dir" rev-parse HEAD)" ]] ||
        fail "the remote $branch of $name is not the pushed commit"
    echo "Pushed $name"
}

# Pushes runa first. Every later failure is finished by running the script again.
push_all() {
    local github_ssh aur_ssh
    github_ssh="$(git config core.sshCommand || echo ssh) -o StrictHostKeyChecking=yes"
    aur_ssh="ssh -o StrictHostKeyChecking=yes"

    push . runa main "$github_ssh"
    HINT="runa is pushed. Run the script again to publish the rest: $0 $VERSION"
    push "$AUR_DIR" AUR master "$aur_ssh"
    push "$BREW_DIR" homebrew-tap main "$github_ssh"
    push "$SCOOP_DIR" scoop-bucket main "$github_ssh"
    HINT=""
}

confirm() {
    local answer
    read -rp "$1" answer
    [[ $answer == "$2" ]]
}

main() {
    [[ $# -eq 1 && $1 =~ $VERSION_RE ]] || fail "usage: $0 <version>, for example 0.12.4"
    VERSION=$1
    TAG="v$VERSION"

    cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
    TMP=$(mktemp -d)
    AUR_DIR=$TMP/aur
    BREW_DIR=$TMP/homebrew-tap
    SCOOP_DIR=$TMP/scoop-bucket
    trap on_exit EXIT

    step "Checking the environment"
    check_environment

    step "Checking the repositories"
    check_runa
    clone_repo homebrew-tap "$BREW_DIR"
    clone_repo scoop-bucket "$SCOOP_DIR"
    check_package_files

    step "Checking the AUR"
    check_aur

    step "Checking release $TAG"
    check_release

    step "Verifying the release assets"
    verify_assets

    HINT="Discard the changes with: git restore packaging/arch"

    step "Updating the packages"
    update_aur
    update_homebrew
    update_scoop

    step "Verifying the packages"
    verify_aur
    verify_homebrew
    verify_scoop
    cp -- "$PKGBUILD" "$SRCINFO" "$AUR_DIR/"

    local dir pending=""
    for dir in . "$AUR_DIR" "$BREW_DIR" "$SCOOP_DIR"; do
        pending+=$(changed_files "$dir")
    done
    if [[ -z $pending ]]; then
        step "All packages are at $VERSION and verified, nothing to do"
        return
    fi
    check_changes

    step "Changes"
    git --no-pager diff
    git -C "$AUR_DIR" --no-pager diff
    git -C "$BREW_DIR" --no-pager diff
    git -C "$SCOOP_DIR" --no-pager diff

    echo
    confirm "Type $VERSION to commit: " "$VERSION" || fail "not committed"

    HINT="Nothing is pushed. Undo a new runa commit with: git reset --keep origin/main"
    step "Committing"
    commit_all

    echo
    confirm "Type push to publish: " push || fail "not pushed"

    step "Publishing"
    push_all

    step "Published runa $VERSION to the AUR, Homebrew and Scoop"
}

main "$@"
