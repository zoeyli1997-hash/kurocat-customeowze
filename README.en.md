# kurocat-customeowze

An unofficial [KuroCat](https://github.com/qianyiai/kurocat) fork for freely drawn, photo-inspired cat companions, customized through MCP. The name blends “customize” with “meow”.

[中文首页](README.md) · [Usage](docs/DIY.md) · [Source installation](docs/INSTALL.md) · [MIT](LICENSE)

**Experimental source-only release. No installers for this fork yet.** macOS Apple Silicon build/tests passed; Windows/Linux and individual AI-client integrations remain unverified. Upstream installers do not include these features.

## What changed

- Import custom outlines, faces, coats and drawing styles as a transparent six-part PNG, retaining 2D typing/mouse/blink actions.
- New MCP tools: `get_custom_skin_guide`, `import_custom_skin`, `update_custom_skin`, `inspect_custom_skin`, `export_custom_skin`.
- Partial rig/crop editing, same-ID artwork reload, portable `.catpack` sharing and original parameter-cat compatibility.
- Default loose crayon/charcoal prompt; each cat retains its own coat and expression. The golden cat is an example only.
- Improved click-purr audio startup; upstream in-app updates disabled to protect custom changes.

Image generation belongs to the connected AI client or external image tool. This app adds no studio UI, image API client or API-key storage. MCP support does not imply image-generation support.

Follow [docs/INSTALL.md](docs/INSTALL.md) to build this repository and connect its binary. Read [AGENTS.md](AGENTS.md) if using an agent for installation. Existing internal binary name and default data directory remain compatible with upstream; protect existing profiles and do not run both applications together.

Based on upstream v5.1.0. Original MIT copyright and third-party notices are retained. See [upstream README archive](README.upstream.md) for original project documentation; its downloads are not this fork.
