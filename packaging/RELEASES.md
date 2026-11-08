# Release flow for runa

The steps for a release of runa, in order. `X.Y.Z` is the new version.
Commands run from the root of the runa repo.

> [!IMPORTANT]
> Only for runa maintainers.  
> Every step that commits, tags, pushes or publishes needs push access to the runa-fm repositories and the AUR packages, be a crates.io crate owner, and signed commits and tags.  
> Fedora also needs admin access to the `runa` COPR project.


---

## 0. Once: Arch

`packaging/scripts/release.sh` needs the Arch tools: `updpkgsums` and `makepkg`.
The script uses toolbox and runs these Arch tools on any Linux host.

Check first that the toolbox exists:

```sh
toolbox list
toolbox run -c runa-arch sh -c 'command -v updpkgsums && command -v makepkg'
```

Only if it's missing, install `toolbox` for your distro then create the container.


| Distro                    | Install                         |
|---------------------------|---------------------------------|
| Fedora                    | sudo dnf install toolbox        |
| Arch                      | sudo pacman -S toolbox          |
| Debian 12+, Ubuntu 22.04+ | sudo apt install podman-toolbox |


```sh
toolbox create --distro arch --release latest runa-arch
toolbox run -c runa-arch sudo pacman -S --needed pacman-contrib
```

---

## 1. Prepare the release

1. Changelog: rename `## [X.Y.Z] - UNRELEASED` to `## [X.Y.Z] - YYYY-MM-DD` and add the `#### <title>.` line.
    - The release workflow uses this section as the release notes.
2. Set `version = "X.Y.Z"` in `Cargo.toml`, then run `cargo check` so `Cargo.lock` updates.
3. Run the checks:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

---

## 2. crates.io dry run

```sh
cargo publish --dry-run
```

---

## 3. Commit, tag and push


```sh
git commit -am "chore(release): vX.Y.Z"
git tag -s vX.Y.Z -m "Release vX.Y.Z: <changelog title>" -m "See CHANGELOG.md for more details."
git push --atomic origin main vX.Y.Z
```

The tag runs the CI, then the release workflow (builds, archives, `SHA256SUMS.txt`, attestations, GitHub release), then the docs deployment.

- Don't delete failed workflow runs on the release commit. It can leave a check stuck "in progress".
- A manual run of `releases.yml` is a dry run. With the GitHub cli: `gh workflow run releases.yml --ref main`
- Publishing an existing tag by hand: `gh workflow run releases.yml --ref vX.Y.Z -f dry_run=false`.

---

## 4. crates.io

Once the GitHub release is out:

```sh
cargo publish
```

---

## 5. AUR, Homebrew, Scoop

Before: runa is clean on `main` and **equal** to `origin/main`.

Run:

```sh
packaging/scripts/release.sh X.Y.Z
```

What it does:

1. Checks the tools, the toolbox, the GitHub cli and runa, then clones `homebrew-tap` and `scoop-bucket` into a temporary directory.
2. Checks that the AUR equals `packaging/arch/` at a commit on main. PKGBUILD changes committed to `packaging/arch/` before the release ship with it.
3. Checks that the release `vX.Y.Z` is published, its tag is a commit on main and every asset is attested and matches `SHA256SUMS.txt`.
4. Updates `packaging/arch/PKGBUILD` and `.SRCINFO`, the Homebrew formula and the Scoop manifest, and checks that only version, url and hash lines changed.
5. Shows the diff and asks you to type `X.Y.Z` to commit. Each repository with changes gets one commit, runa gets `chore(packaging): update aur package to X.Y.Z`. The commits **MUST** be signed.
6. Asks you to type `push`, then pushes runa first, then the AUR, Homebrew and Scoop.

With a version that is already packaged, it only verifies and ends with "nothing to do".

If the release stops:

- Before committing: discard with `git restore packaging/arch`. The clones are temporary and need no cleanup.
- After committing, before pushing: undo the runa commit with `git reset --keep origin/main`.
- After runa is pushed: run `packaging/scripts/release.sh X.Y.Z` again. It skips what is already published and finishes the rest.

The script prints the right command when it stops.

---

## 6. Fedora (COPR)

Not part of the script. Packit builds the `runa` COPR project from the published GitHub release, using `.packit.yaml` and `packaging/fedora/runa.spec`.

1. Check that the build for `vX.Y.Z` succeeds on the COPR project page.
2. The first release build asks for builder permission for `packit`. Approve it under the project's Permissions.

---

## 7. runa-bin (AUR, by hand)

Not part of the script. Clone the AUR package first.

1. Set `pkgver=X.Y.Z` and `pkgrel=1` in the PKGBUILD.
2. Update the hashes and `.SRCINFO`:

```sh
updpkgsums
makepkg --printsrcinfo > .SRCINFO
```

If toolbox is needed:

```sh
toolbox run -c runa-arch updpkgsums
toolbox run -c runa-arch makepkg --printsrcinfo > .SRCINFO
```

3. Copy the changelog from the tag:

```sh
git -C <path/to/runa> show vX.Y.Z:CHANGELOG.md > CHANGELOG.md
```

4. Commit and push to the AUR.

---

## 8. Next version

1. Set `version = "X.Y.(Z+1)-alpha.1"` in `Cargo.toml` and run `cargo check`.
2. Commit:

```sh
git commit -am "chore(bump): runa version to X.Y.(Z+1)-alpha.1"
```

3. Add `## [X.Y.(Z+1)] - UNRELEASED` to the changelog with the first change, not before.
