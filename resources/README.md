# Keyzen 应用图标资源

本目录包含不同平台的应用图标资源。

## 目录结构

```
resources/
├── macos/
│   ├── keyzen.icns          # macOS 图标文件
│   └── AppIcon.iconset/     # 图标源文件
├── linux/
│   ├── keyzen.png           # 主图标 (512x512)
│   ├── keyzen-256.png
│   ├── keyzen-128.png
│   ├── keyzen-64.png
│   ├── keyzen-48.png
│   ├── keyzen-32.png
│   └── keyzen-16.png
└── windows/
    └── keyzen.png           # Windows 图标源文件 (1024x1024)
```

## 平台特定说明

### macOS

1. **自动识别**：GPUI 通过 `app_id` ("dev.keyzen.Keyzen") 自动查找系统图标
2. **打包到 App Bundle**：
   - 将 `keyzen.icns` 复制到 `Keyzen.app/Contents/Resources/`
   - 在 `Info.plist` 中设置：
     ```xml
     <key>CFBundleIconFile</key>
     <string>keyzen</string>
     ```

### Linux

1. **桌面文件**：创建 `~/.local/share/applications/keyzen.desktop`：
   ```ini
   [Desktop Entry]
   Type=Application
   Name=Keyzen
   Comment=键禅 - 打字练习工具
   Icon=/path/to/keyzen/resources/linux/keyzen.png
   Exec=/path/to/keyzen
   Categories=Education;
   Terminal=false
   ```

2. **系统图标**：将不同尺寸的图标安装到：
   - `/usr/share/icons/hicolor/512x512/apps/keyzen.png`
   - `/usr/share/icons/hicolor/256x256/apps/keyzen-256.png`
   - 等等...

### Windows

1. **生成 .ico 文件**：
   - 使用工具（如 ImageMagick、在线转换器）将 `keyzen.png` 转换为 `keyzen.ico`
   - 包含多个尺寸：16x16, 32x32, 48x48, 64x64, 128x128, 256x256

2. **嵌入到可执行文件**：
   - 使用 `winres` crate 在编译时嵌入图标
   - 或使用资源编辑器后期添加

## 开发说明

- 原始图标：`~/Downloads/keyzen_logo.png` (1024x1024)
- App ID：`dev.keyzen.Keyzen`
- 已在 `main.rs` 的 `WindowOptions` 中设置 `app_id`

## 更新图标

如需更新图标，运行：

```bash
# macOS
cd resources/macos
rm -rf AppIcon.iconset keyzen.icns
# 重新生成（参考上述步骤）

# Linux
cd resources/linux
# 重新生成不同尺寸

# Windows
cd resources/windows
# 更新源文件并重新转换
```
