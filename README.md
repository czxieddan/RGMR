<div align="center">

<p align="center">
  <img src="https://i.imgur.com/d7PUXzu.png" alt="app" />
</p>

# RGMR

面向 OpenAI 兼容视觉模型的 AI 地理识图定位桌面应用

从一张图片中提炼地理线索，将大模型的自由文本输出整理为清晰、可复核的结构化定位结果，并围绕 Windows 桌面习惯打磨粘贴、拖拽与结果浏览体验。

[![Stars](https://img.shields.io/github/stars/czxieddan/RGMR?style=flat-square)](https://github.com/czxieddan/RGMR/stargazers)
[![License](https://img.shields.io/github/license/czxieddan/RGMR?style=flat-square)](LICENSE)
[![Release](https://img.shields.io/github/v/release/czxieddan/RGMR?style=flat-square)](https://github.com/czxieddan/RGMR/releases)
[![Rust](https://img.shields.io/badge/Rust-2024-000000?style=flat-square&logo=rust)](https://www.rust-lang.org/)

</div>

<p align="center">
  <img src="https://i.imgur.com/aKYLkDl.png" alt="RGMR 主展示图" width="100%" />
</p>

## 项目简介

RGMR 是一个基于 Rust 构建的桌面应用，聚焦于图片地理定位这一高价值视觉任务：

- 接入 OpenAI 兼容视觉模型
- 接收剪贴板图片与拖拽导入内容
- 发起识图定位请求
- 将模型输出解析为更适合人类判断与后续处理的分层结果

它并不是一个模型提供方，而是一个面向实际工作流的桌面前端工具：你提供兼容的 API 服务、模型与密钥，RGMR 负责把导入、请求、解析、展示这一整条链路收束到一个顺手的 Windows 桌面界面中。

## 项目亮点

### 1. 面向 OpenAI 兼容生态的接入方式

- 支持自定义 Base URL、API Key、模型名称与请求超时
- 可直接拉取模型列表，减少手动填写模型名的成本
- 针对 OpenAI 兼容服务做了更宽容的端点规整，会自动尝试常见的 `/v1`、`/chat/completions`、`/models` 等候选路径
- 请求头同时兼容常见 Bearer、`api-key`、`x-api-key` 网关风格

### 2. 为高频识图操作优化的输入体验

- 支持 `Ctrl+V` 粘贴剪贴板图片
- 支持将图片直接拖拽到窗口中导入
- 在配置完整时，粘贴后可直接进入定位流程，减少重复点击
- Windows 下提供更贴近原生桌面习惯的粘贴监听体验

### 3. 结构化的定位结果，而不是只给一段原始文本

RGMR 会引导模型按层级输出结果，并将返回内容解析为以下结构：

- 大洲某方位某国
- 国内区位大区
- 具体城市区位
- 具体地点

同时保留：

- 解析状态 `Strict / Partial / Fallback`
- 置信说明
- 原始输出文本

这使它不仅适合直接查看，也适合后续复制、复核与纳入更大的工作流。

### 4. 更像原生应用的桌面体验

- 基于 Rust 与 `eframe/egui` 构建
- 深色风格、桌面窗口化交互、拖拽导入支持完整
- 配置会自动保存到本地系统配置目录
- 内置多语言界面能力，当前包含简体中文、English、Русский

## 界面展示

### 主界面

<p align="center">
  <img src="https://i.imgur.com/aKYLkDl.png" alt="RGMR 主界面" width="100%" />
</p>

<p align="center"><sub>配置区、图片导入区与结构化结果面板集中在同一桌面工作流中。</sub></p>

### 精准度

<p align="center">
  <img src="https://i.imgur.com/JKJJwwD.png" alt="RGMR 精准度" width="100%" />
</p>

<p align="center"><sub>模型输出会被整理为更便于阅读与判断的地理定位层级结果。</sub></p>

## 适用场景

RGMR 适合以下类型的使用场景：

- 地理识图、街景判断、旅行照片位置回溯
- 视觉模型地理定位能力测试与效果对比
- 需要把自由文本回答沉淀为结构化层级结果的工作流
- 需要在桌面环境中高频粘贴截图、快速验证位置线索的个人或团队使用方式

## 快速开始

### 环境要求

- Rust 与 Cargo
- 推荐 Windows 10 / 11
- 一个可用的 OpenAI 兼容视觉模型服务
- 对应服务的 API Key

### 获取项目

```bash
git clone https://github.com/czxieddan/RGMR.git
cd RGMR
```

### 本地运行

```bash
cargo run --release
```

程序启动后，先完成 API 配置，再导入图片开始定位。

## 配置说明

### Base URL

- 默认值为 `https://api.openai.com/v1`
- 你可以填写服务根地址、`/v1` 地址
- 程序会自动规整并尝试推导可用的聊天补全与模型列表端点

### API Key

- 用于请求你所配置的 OpenAI 兼容服务
- 当前版本默认将密钥保存在本地配置文件中
- 如果你在共享设备或公共环境使用，请自行做好系统权限与文件访问控制

### 模型列表

- 可通过界面中的刷新操作拉取远端模型列表
- 如果当前模型不在拉取结果中，程序会自动切换到可用模型
- 在某些兼容服务无法稳定返回模型列表时，也可启用手动模型名兜底方式

### 图片输入

- `Ctrl+V` 可以直接读取剪贴板图片
- 支持拖拽图片到窗口导入
- 导入后可直接发起定位
- 常用快捷键：`Ctrl+V` 读取图片，`Ctrl+Enter` 发起定位

### 提示词与结果约束

- 内置默认系统提示词，目标是引导模型输出稳定的层级化地理定位结果
- 支持按界面语言同步默认提示词
- 支持恢复默认提示词
- 支持开启或关闭置信说明输出
- 如果你深度改写提示词，可能会削弱结构化解析的稳定性

### 超时与配置

- 请求超时支持在 `10` 到 `180` 秒之间调整
- 配置会自动保存到系统配置目录下的 `RGMR/config.toml`

## 使用流程

1. 启动应用并填写 Base URL 与 API Key
2. 刷新模型列表，或手动指定模型名
3. 通过 `Ctrl+V` 或拖拽方式导入图片
4. 点击开始定位，或使用 `Ctrl+Enter` 发起请求
5. 查看结构化结果、解析状态、置信说明与原始输出
6. 根据需要继续替换图片并重复分析

## 构建说明

### 调试构建

```bash
cargo build
```

### 发布构建

```bash
cargo build --release
```

发布产物默认位于：

```text
target/release/rgmr.exe
```

项目在 Windows 下包含资源嵌入逻辑，会在构建过程中写入应用图标与版本信息，以获得更完整的桌面程序呈现效果。

## 许可证

<a href="https://gnu.ac.cn/licenses/agpl-3.0.html">
 <img src=https://gnu.ac.cn/graphics/agplv3-with-text-162x68.png alt="GNU Affero General Public License v3.0 (AGPL v3)">
</a>

本项目采用 GNU Affero General Public License v3.0 许可证发布。详情请参阅 [`LICENSE`](LICENSE)。

## 说明

- RGMR 自身不提供模型服务，也不附带 API Key
- 导入的图片会随请求发送至你配置的 OpenAI 兼容服务，隐私安全不由 RGMR 保障
