# Publishing to GitHub

## Source repository

1. Create an empty GitHub repository with your chosen name. Leave GitHub's initial README, license, and `.gitignore` options unchecked.
2. From this project's root, initialize Git and inspect the files before committing:

   ```powershell
   git init -b main
   git add .
   git status --short
   git diff --cached --stat
   ```

   Include `.cargo`, `.github`, `crates`, `docs`, `installer`, `xtask`, both Cargo files, and root documentation/configuration files. Keep `target`, `dist`, local runtime data, and credentials out of the commit. `.gitignore` applies to Git; it does not filter files dragged into GitHub's browser uploader.

3. Commit and push using the repository URL GitHub provides:

   ```powershell
   git commit -m "Initial public release"
   git remote add origin https://github.com/YOUR-USERNAME/YOUR-REPOSITORY.git
   git push -u origin main
   ```

4. Confirm the Windows CI workflow passes. Suggested repository description: **Windows desktop app for setting up and managing BOIII Zombies dedicated servers, built with Rust.** Suggested topics: `rust`, `windows`, `boiii`, `black-ops-3`, `zombies`, `dedicated-server`.
5. Under **Settings > General > Social preview**, upload `assets/branding/social-preview.png` (1280 x 640) so links to the repository show the banner.

## Release assets

1. Run `cargo test --workspace --locked` and `cargo xtask package` on Windows.
2. Smoke-test the generated installer and installed application. Record which in-game checks were actually performed; see [testing notes](TESTING.md).
3. Create a version tag matching `Cargo.toml` (currently `v0.51.0`) and a GitHub Release describing changes and known limitations.
4. Attach `dist\BoiiiServerMakerSetup.exe` to the release rather than committing it. Optionally publish its SHA-256 checksum using `Get-FileHash .\dist\BoiiiServerMakerSetup.exe -Algorithm SHA256`.

Verify the origin and redistribution terms of bundled third-party assets, including the community lobby script, before public distribution. Do not include game files or downloaded BOIII client binaries.
