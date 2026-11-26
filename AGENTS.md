# Instructions for agents using this fork

This repository is `zoeyli1997-hash/kurocat-customeowze`, an unofficial KuroCat fork. Read `README.md`, `docs/INSTALL.md` and `docs/DIY.md` before installation or customization.

- Install/build THIS repository's default branch; upstream KuroCat binaries do not include its custom-artwork tools.
- No fork installers are currently published. Do not invent a Homebrew/npm package or download upstream installers as a substitute.
- Inspect the user's OS and existing installation. Protect existing cats/settings, and avoid overwriting or running beside their current KuroCat. Internal app/binary name and default data directory still match upstream.
- For isolated development, `KUROCAT_DATA_DIR` accepts an absolute path. Use the same value for the GUI and MCP process. Do not commit user profiles, raw reference photos or credentials.
- After dependencies: `npm ci`, `npm run build`, `npm run tauri -- build --no-bundle`. The MCP binary is `src-tauri/target/release/KuroCat` (Windows: `KuroCat.exe`), invoked with `--mcp`.
- Confirm `tools/list` contains `get_custom_skin_guide` and `import_custom_skin`; mere application launch is insufficient. Configure the client's MCP to the newly built binary's absolute path.
- Read `get_custom_skin_guide` before generating artwork. Use the user's own cat identity; do not substitute the golden example. Image generation must be provided by the client's tools or an external service. Never imply MCP itself generates images.
- Custom art is 2D. Import explicit local PNG/catpack paths; adjust with `update_custom_skin`. Use `upsert_cat` only for original parameter cats. Export catpacks for sharing. Do not change the active pet when the user requested previews only.
- macOS Accessibility/Input Monitoring may require the user to authorize the actual executable/app. Do not claim these permissions were verified without observing input events.
- Tests: `node --test tests/audio-latency.test.mjs tests/diy-skin.test.mjs`; `cargo test --manifest-path src-tauri/Cargo.toml --lib --release`; then `KUROCAT_TEST_BINARY=src-tauri/target/release/KuroCat node --test tests/mcp-custom-skins.test.mjs`.
