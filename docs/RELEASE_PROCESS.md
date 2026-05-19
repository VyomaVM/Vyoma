# Vyoma Release Process

This document outlines the standard operating procedure for bumping the version number of the Vyoma project across the entire repository. This ensures that the daemon, UI, build scripts, and documentation all remain perfectly in sync before cutting a new release.

## Version Locations

Whenever you bump the version, you must update the version string in the following files:

1. **`Cargo.toml`** (Workspace Root)
   - *Line:* `[workspace.package] version = "..."`
   - *Purpose:* This dictates the version for the Rust binaries (`vyomad` and `vyoma`). Thanks to our recent update, this value is automatically injected into the frontend UI, so no React component needs manual updating.

2. **`packaging/deb/build.sh`**
   - *Line:* `VERSION="..."`
   - *Purpose:* Sets the package version for Debian/Ubuntu (`.deb`) builds.

3. **`packaging/rpm/build.sh`**
   - *Line:* `VERSION="..."`
   - *Purpose:* Sets the package version for RHEL/CentOS/Fedora (`.rpm`) builds.

4. **`README.md`**
   - *Lines:* The installation command examples.
   - *Purpose:* Ensures the quick-start instructions point to the newly released `.deb` and `.rpm` files.
   - *Example:* `sudo apt install ./vyoma_X.X.X_amd64.deb`

5. **`ui/package.json`** (Optional but Recommended)
   - *Line:* `"version": "..."`
   - *Purpose:* Keeps the npm package manifest aligned with the overall project version.

## GitHub Actions Workflows

The `.github/workflows/release.yml` automatically reads the tag version and invokes the `packaging/deb/build.sh` and `packaging/rpm/build.sh` scripts. You do not need to update any `.yml` files, you only need to update the bash scripts listed above.

## Steps for a Release

1. Update the version strings in the 5 locations above.
2. Run `npm install` inside the `ui` folder if `package.json` was changed, ensuring `package-lock.json` is updated.
3. Commit the changes: `git commit -m "chore: bump version to vX.X.X"`.
4. Create an annotated git tag: `git tag -a vX.X.X -m "Release vX.X.X"`.
5. Push the commit and the tag to GitHub: `git push origin main && git push origin vX.X.X`.
6. The `release.yml` workflow will automatically trigger, build the UI, compile the daemon, package the `.deb` and `.rpm` files, and attach them to a new GitHub Release.
