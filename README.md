# Keyzen - 键禅

> 基于 Rust + GPUI 的跨平台打字练习软件

**键起，心流生** - Type with Flow

## 特性

- 🧘 **禅意心流**：极简界面，零干扰的专注体验
- ⚡ **极致性能**：120FPS+ 流畅渲染，零延迟反馈
- 🌍 **跨越语言**：支持中文（含拼音）、英文、代码等多语言
- 📊 **数据驱动**：科学的进步追踪与个性化推荐

## 项目状态

当前处于 **Phase 1 (MVP)** 开发阶段。

### 已完成

- ✅ 项目架构设计（完整设计文档：`DESIGN.md`）
- ✅ Workspace 结构搭建
- ✅ `keyzen_core` - 核心类型定义
- ✅ `keyzen_data` - 课程加载系统（RON 格式）
- ✅ `keyzen_engine` - 打字逻辑引擎（Forgiving 模式）
- ✅ `keyzen_ui` - 终端 UI（临时方案）
- ✅ 示例课程（5 个：英文 2 个 + Rust 代码 3 个）
- ✅ WPM/准确率实时计算
- ✅ 薄弱按键分析

### 下一步

- [ ] 完整的 GPUI 图形界面
- [ ] 中文拼音输入支持
- [ ] 数据持久化（SQLite）
- [ ] 主题系统
- [ ] 更多课程内容

## 快速开始

### 安装依赖

确保已安装 Rust 工具链：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 运行项目

```bash
# 编译所有 crates
cargo build --workspace

# 运行终端版 Keyzen
cargo run --bin keyzen
```

### 项目结构

```
keyzen/
├── DESIGN.md                  # 完整设计文档
├── Cargo.toml                 # Workspace 配置
├── crates/
│   ├── keyzen_core/          # 核心类型定义
│   ├── keyzen_data/          # 课程数据加载
│   ├── keyzen_engine/        # 打字逻辑引擎
│   └── keyzen_ui/            # 用户界面
└── lessons/                   # 课程文件（RON 格式）
    ├── english/              # 英文课程
    └── code/rust/            # Rust 代码课程
```

## 架构设计

Keyzen 采用三层架构，实现数据与程序分离：

```
表现层 (keyzen_ui)
    ↓
业务逻辑层 (keyzen_engine)
    ↓
基础设施层 (keyzen_core + keyzen_data)
```

详细设计请参阅 [`DESIGN.md`](./DESIGN.md)。

## 添加自定义课程

课程使用 RON 格式定义，示例：

```ron
Lesson(
    id: 1,
    lesson_type: Prose,
    language: "en-US",
    title: "你的课程标题",
    description: "课程描述",
    source_text: "练习文本内容",
    meta: LessonMeta(
        difficulty: Beginner,
        tags: ["标签1", "标签2"],
        estimated_time: (secs: 60, nanos: 0),
        prerequisite_ids: [],
    ),
)
```

将文件保存到 `lessons/` 目录下即可自动加载。

## 测试

```bash
# 运行所有测试
cargo test --workspace

# 运行特定 crate 的测试
cargo test -p keyzen_engine
```

## 贡献

欢迎贡献！请参考以下方式：

1. 创建课程内容（放入 `lessons/` 目录）
2. 报告 Bug 或提出建议
3. 提交 Pull Request

## 许可证

MIT OR Apache-2.0

## 致谢

- 灵感来源：[Zed Editor](https://zed.dev)
- UI 框架：GPUI（Zed 底层库）
- 设计理念：禅宗美学 + 心流理论
