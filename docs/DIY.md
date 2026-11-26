# 在 AI 聊天里定制自己的小猫

这是基于 KuroCat 的非官方扩展。沿用原来的 MCP 连接配置，不增加独立工坊窗口，也不在应用里配置图像服务或密钥。

## 怎么使用

1. 在支持 MCP 的 AI 客户端里连接 KuroCat：右键小猫 → 开启 MCP 扩展 → 复制 MCP 配置，按客户端的配置方式接入。
2. 发自己猫咪的照片，告诉 AI：“先读取 KuroCat 的 get_custom_skin_guide，按照片做一只随性手绘小猫，保留它的神态和花色。”
3. AI 使用它自己的图像工具生成透明动作素材，保存到运行 KuroCat 的电脑，然后调用 import_custom_skin。默认立即显示在桌面。
4. 继续说“头缩小一点”“爪子短一点”，AI 可以直接调整比例；“眼神再呆一点”“线条粗糙一点”等外观变化需要 AI 重新绘制素材并更新。桌面上的同一只猫会自动加载新版。
5. 满意后让 AI 导出 .catpack 文件。朋友可以通过同样的 MCP 工具导入，不用再生成一次。

支持 MCP 不代表客户端一定能生成图片。没有图像工具时，可在外部生成并把文件保存到本机，再让 AI 导入。KuroCat 自身不生成图片、不调用付费图像接口；图像服务的费用及照片处理规则由所使用的 AI 客户端或外部工具决定。

## 自由度与默认画风

原版 get_cat_schema / upsert_cat 保留，用于改已有模型的颜色、花纹和斑点。新增素材工具允许重新绘制脸型、身材、耳形、毛色、神态和笔触。

默认提示词采用松散蜡笔、断线重描、略怪诞的手绘感，卡通程度约 7 分。毛色、花纹、瞳色、耳形和神态由每个人自己的照片决定，没有固定金色或灰色毛尖。你家的金渐层只放在 examples/ 里做示例，不作为默认皮肤。需要时也可以让 AI 改画风，导入检查的是素材结构，不是颜色或艺术风格。

## MCP 工具

| 工具 | 用途 |
|---|---|
| get_custom_skin_guide | 获取完整画风提示词、动作布局、限制和操作流程 |
| import_custom_skin | 从本机绝对路径导入透明 PNG 或 .catpack；PNG 自动生成编号；可立即使用 |
| update_custom_skin | 修改名称、头身爪比例、头部位置、裁切区域，或更换整张素材 |
| inspect_custom_skin | 获取配置及素材 PNG；返回的是部件图，不是组装后的动画截图 |
| export_custom_skin | 导出自包含皮肤包，默认不覆盖已存在的文件 |
| list_cats / switch_cat / delete_cat | 列表、切换和移除；同时支持原版参数猫和新素材猫 |

导入已有编号默认拒绝覆盖。明确更新时用 update_custom_skin，或导入时传 overwrite:true。复制为另一只猫时指定新的 diy- 编号。修改素材猫不能使用原版 upsert_cat，以免把素材配置覆盖成参数猫。

## 素材要求与边界

- 单张真正透明的 PNG，2 列 × 3 行，依次为：睁眼头、闭眼头、身体、尾巴、左前爪、右前爪。每个部件要有透明留白，不能接触邻格。
- 图片尺寸每边 64–4096 像素，PNG 最大 10 MB，皮肤包最大 20 MB，小猫库最多 64 只。
- 裁切 x/y/w/h 使用 0–1 相对坐标，可以逐字段修改，不用重复发送整个配置。
- 头、身体、爪子缩放范围 0.65–1.4；头部左右和上下偏移范围 -35–35。整体桌宠大小仍可用原版 update_settings 的 scale 调整。
- 自定义素材使用 2D，保留现有键盘、鼠标、眨眼等动作；不会自动生成 3D 模型或任意新骨骼动作。
- 文件必须在运行 KuroCat 的电脑上，使用明确的绝对路径。不接受远程 URL，也不需要把大段 base64 贴进聊天。远程 AI 客户端需先把文件传到这台电脑。

## 保存和分享

皮肤包只含绘制后的 PNG、名称、裁切和动作设置，不含参考照片、API 密钥或可执行脚本。文件存储在应用数据目录的 diy-skins/，与程序文件分开；删除时移入该目录的 .trash/。更新素材会保持猫的编号，并刷新桌面及 OBS 的素材。

导出时指定本机 .catpack 文件的绝对路径。父目录需要存在；已有文件只有明确传 overwrite:true 才会覆盖。

## 开发验证

```text
node --test tests/audio-latency.test.mjs tests/diy-skin.test.mjs
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib --release
cargo build --manifest-path src-tauri/Cargo.toml --release --bin KuroCat
KUROCAT_TEST_BINARY=src-tauri/target/release/KuroCat node --test tests/mcp-custom-skins.test.mjs
```

集成测试用 KUROCAT_DATA_DIR 指向临时绝对路径，隔离真实用户的小猫和设置。该变量不设置时沿用原应用数据目录。测试不生成新图片、不访问付费服务。
