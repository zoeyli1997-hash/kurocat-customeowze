# 从这个 Fork 安装

仓库：https://github.com/RockyYang1225/kurocat-customeowze

请使用默认分支 `customeowze`。当前发布的是源码，不是安装包；原版 KuroCat 的安装包不会包含新增的素材工具。

## 安装前

- 首版只验证了 Apple Silicon Mac 上的构建、测试与素材渲染。Windows / Linux 还需分别验证。
- 需要 Git、Node.js 22.18+（推荐 24 LTS）、npm、Rust stable/Cargo。macOS 还需 Xcode Command Line Tools；其他系统需要 [Tauri 官方前置依赖](https://v2.tauri.app/start/prerequisites/)。
- 内部程序名暂沿用 KuroCat，默认数据目录也沿用上游。已有小猫时先备份用户数据，不要直接覆盖旧应用，不要同时运行两个版本。
- macOS 默认数据目录是 `~/Library/Application Support/kuro-cat-pet/`。其他平台参见源码中的 `config::app_data_dir`。
- 如需隔离测试，可设置 `KUROCAT_DATA_DIR` 为单独的绝对目录。GUI 和 MCP 进程必须使用同一目录，否则彼此看不到保存的小猫。不要把测试目录提交到 Git。

## macOS 源码构建

```sh
git clone https://github.com/RockyYang1225/kurocat-customeowze.git
cd kurocat-customeowze
npm ci
npm run tauri -- build --no-bundle
```

构建使用当前仓库的前端资源，输出 `src-tauri/target/release/KuroCat`。先测试和运行这个程序即可，不要求制作 DMG 或复制到 /Applications。不要用单独的 `cargo build` 来代替桌面构建：它可用于 MCP 测试，但未必包含桌面前端资源。

开发预览可使用 `npm run tauri:dev`。

## 连接 MCP

在支持本地 MCP 的 AI 客户端里添加以下配置，替换 `command` 为真实绝对路径。不要把示例占位路径原样照抄。

```json
{
  "mcpServers": {
    "kurocat-customeowze": {
      "command": "/absolute/path/to/kurocat-customeowze/src-tauri/target/release/KuroCat",
      "args": ["--mcp"]
    }
  }
}
```

隔离测试时，在这个 server 下增加 `env`：

```json
{"KUROCAT_DATA_DIR": "/absolute/path/to/separate-cat-profile"}
```

GUI 也要从设置同一环境变量的环境启动。使用图形化打包应用时环境变量不一定自动继承，请按实际启动方式检查。

运行中的 GUI 还提供本机 `http://127.0.0.1:27420/mcp`，需开启 MCP 扩展。此地址只表示当前电脑；云端 Agent 不一定能访问，不要把它当成公网地址。

## 验证装对了版本

让 AI 列出 MCP 工具，确认有 `get_custom_skin_guide`、`import_custom_skin`、`update_custom_skin`、`inspect_custom_skin`、`export_custom_skin`。只有 `upsert_cat` 等原版工具，说明很可能接到了旧版程序。

macOS 真实键鼠响应还需要为实际运行的程序授予辅助功能、输入监控等权限。源码构建成功不等于这些权限已验证。

## 开始做自己的猫

见 [MCP 定制说明](DIY.md)。图片生成由所用 AI 客户端提供；不能生图的客户端可以导入外部生成的素材。`.catpack` 是小猫成品文件，需安装此分支后导入，不能双击当作应用运行。

一段给朋友复制到 Agent 的话：

> 请安装 https://github.com/RockyYang1225/kurocat-customeowze 的默认分支。先读 README.md、AGENTS.md 和 docs/INSTALL.md，检查本机编译环境，从源码构建并配置新增的 MCP 工具。不要用原版安装包代替；保护已有小猫和设置。遇到需要登录或系统授权的步骤请让我完成。
