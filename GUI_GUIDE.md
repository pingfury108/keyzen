# Keyzen GUI 开发指南

## 双版本架构

Keyzen 现在支持两个独立的用户界面：

### 1. TUI 版本（keyzen-tui）
- **框架**：Ratatui 0.29 + Crossterm 0.28
- **特点**：轻量、快速、跨平台
- **适用**：终端用户、服务器环境、远程开发
- **构建**：`cargo build --bin keyzen-tui`

### 2. GUI 版本（keyzen-gui）
- **框架**：GPUI (Zed Editor 底层框架)
- **特点**：120 FPS+、GPU 加速、现代化界面
- **适用**：桌面用户、追求极致体验
- **构建**：`cargo build --bin keyzen-gui`

## GPUI 配置说明

### 系统要求

#### macOS
```bash
# 需要 Xcode Command Line Tools
xcode-select --install
```

#### Linux
```bash
# Ubuntu/Debian
sudo apt install libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libfontconfig1-dev

# Arch Linux
sudo pacman -S libxcb libxkbcommon fontconfig
```

#### Windows
- 需要安装 Visual Studio Build Tools
- 暂不完全支持，建议使用 WSL2

### 编译时间

GPUI 是一个大型依赖，首次编译可能需要：
- **macOS M1/M2**：10-15 分钟
- **Linux (8核)**：15-20 分钟
- **Windows WSL2**：20-30 分钟

### 常见问题

#### 1. 编译卡住

GPUI 编译时占用大量内存，如果编译卡住：

```bash
# 限制并行编译任务
cargo build --bin keyzen-gui -j 2

# 或使用 release 模式（更快）
cargo build --bin keyzen-gui --release
```

#### 2. 链接错误

如果遇到链接错误，确保安装了所有系统依赖。

macOS:
```bash
brew install pkg-config
```

Linux:
```bash
# 安装开发库
sudo apt install build-essential pkg-config
```

#### 3. 运行时黑屏

GPUI 需要 GPU 支持，如果运行时黑屏：
- 检查显卡驱动是否最新
- 尝试设置环境变量：`RUST_LOG=debug cargo run --bin keyzen-gui`

## 开发建议

### 快速迭代

开发时建议使用 TUI 版本进行快速测试：

```bash
# TUI 版本编译快（~2秒）
cargo run --bin keyzen-tui

# GUI 版本编译慢（首次 10+ 分钟）
cargo run --bin keyzen-gui
```

### 共享逻辑

两个版本共享核心逻辑：
- `keyzen_core`：类型定义
- `keyzen_engine`：打字引擎
- `keyzen_data`：课程加载

修改核心逻辑会同时影响两个版本。

## 当前状态

### ✅ 已完成
- [x] TUI 版本（完全可用）
- [x] 双版本架构设计
- [x] GUI 基础代码框架

### 🚧 进行中
- [ ] GPUI 编译（首次需要较长时间）
- [ ] GUI 界面调试

### 📋 待实现
- [ ] GUI 主题系统
- [ ] GUI 动画效果
- [ ] 字形缓存优化
- [ ] 多窗口支持

## 使用建议

**现阶段推荐使用 TUI 版本进行打字练习**，GUI 版本仍在开发中。

TUI 版本已经实现了所有核心功能：
- ✅ 实时 WPM/准确率统计
- ✅ Forgiving 输入模式
- ✅ 薄弱按键分析
- ✅ 课程完成展示
- ✅ 稳定的 Ratatui 界面

GUI 版本将在编译完成后进行调试和优化。
