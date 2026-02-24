# MacNfcTool (Tauri + React + libnfc)

一个基于 `Tauri + React` 的 PN532 NFC 工具，支持：
- 连接 PN532 读卡器
- 读取卡片 UID/ATQA/SAK/类型
- 读取 Mifare Classic 1K 扇区数据并预览 HEX + ASCII
- 写入指定扇区块（16 字节）

## 运行前依赖

1. 下载 libnfc 源码到项目目录

目录结构要求：

```text
MacNfcTool/
  third_party/
    libnfc/   # libnfc 源码根目录
```

2. 安装构建工具（macOS）

```bash
brew install automake autoconf libtool pkg-config
```

3. 安装 Node、Rust、Tauri CLI 依赖（首次）

```bash
npm install
```

## 一键编译运行

```bash
./run.sh
```

如果需要指定连接串（例如 PN532 UART）：

```bash
./run.sh "pn532_uart:/dev/tty.usbserial-xxxx"
```

排查设备发现问题：

```bash
LIBNFC_AUTO_SCAN=true LIBNFC_INTRUSIVE_SCAN=true \
  ./third_party/libnfc/build/install/bin/nfc-scan-device
```

`run.sh` 会自动：
- 编译并安装 `third_party/libnfc` 到 `third_party/libnfc/build/install`
- 设置 `LIBNFC_INCLUDE_DIR` / `LIBNFC_LIB_DIR`
- 运行 `cargo check`
- 启动 `npm run tauri:dev`

## 构建发布

```bash
npm run tauri:build
```

## 技术实现说明

- 前端：React + TypeScript，通过 `@tauri-apps/api/core` 调用后端命令。
- 后端：Rust Tauri 命令层 + C bridge。
- C bridge：直接调用 `libnfc` C API 处理 PN532、寻卡、Mifare 认证、读写块。

## 注意事项

- 当前示例只实现了 `Mifare Classic 1K (sector 0-15)`。
- 写块前会用 `key A/B` 对扇区首块认证。
- 某些卡片/扇区可能使用非默认密钥，需要手动输入正确密钥。
