# Lucky

A desktop companion app featuring a pixel panda that lives on your screen.

一个桌面陪伴应用，像素熊猫将陪伴在你的屏幕上。

## Features / 功能

- **Transparent frameless window** — only the panda is visible, no title bar or borders
- **Always on top** — the panda stays above all other windows
- **Draggable** — click and drag the panda to move it anywhere on your desktop
- **Idle animation** — gentle breathing/bouncing motion

---

- **透明无边框窗口** — 只显示熊猫，没有标题栏和边框
- **始终置顶** — 熊猫始终显示在其他窗口上方
- **可拖拽** — 点击拖拽熊猫移动到桌面任意位置
- **待机动画** — 轻柔的呼吸弹跳效果

## Tech Stack / 技术栈

- [Tauri v2](https://tauri.app/) — Rust backend + native window
- [React 19](https://react.dev/) + TypeScript — frontend UI
- [Vite](https://vite.dev/) — frontend bundler
- CSS box-shadow pixel art — pixel panda rendering

## Prerequisites / 前置要求

- [Bun](https://bun.sh/)
- [Rust](https://www.rust-lang.org/tools/install)

## Getting Started / 快速开始

```bash
# Install dependencies / 安装依赖
bun install

# Run in development mode / 开发模式运行
bun run tauri dev

# Build for production / 生产构建
bun run tauri build
```

## Project Structure / 项目结构

```
src/              # React frontend / React 前端
  App.tsx         # Panda component / 熊猫组件
  App.css         # Pixel art & animation / 像素画和动画
src-tauri/        # Rust backend / Rust 后端
  src/lib.rs      # Tauri app entry / Tauri 应用入口
  tauri.conf.json # Window & app config / 窗口和应用配置
```

## License / 许可证

MIT
