# Packaging

Packaging for [runa](https://github.com/runa-fm/runa).

- `arch/`: the PKGBUILD for the [runa](https://aur.archlinux.org/packages/runa) AUR package.
- `fedora/`: the RPM spec for the [COPR](https://copr.fedorainfracloud.org/coprs/alexm-dev/runa/) builds, built by Packit from each release.
- `release.sh`: updates the AUR package, the [Homebrew tap](https://github.com/runa-fm/homebrew-tap) and the [Scoop bucket](https://github.com/runa-fm/scoop-bucket) to a release.
    - What it checks and does is in `RELEASES.md`, step 5.
- `RELEASES.md`: the release flow.

## License
This project is dual licensed under the MIT OR Apache-2.0 license.  

See the [LICENSE-MIT](../LICENSE-MIT) and [LICENSE-APACHE](../LICENSE-APACHE) files for details.
