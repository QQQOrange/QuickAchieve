# Quick Achieve v2.23.125

> 新增微信 OCR（本地）：不填 Key、不联网、不上传图片。

## 怎么接的

微信 PC 端自带的 `WeChatOCR.exe` 没有命令行接口，官方用法是宿主进程通过
`mmmojo_64.dll` 起子进程，再用 protobuf 在管道上收发消息。本项目按公开的
`kanadeblisst00/wechat_ocr` 封装，把同一套调用翻译成 Rust：

1. `InitializeMMMojo` → `CreateMMMojoEnvironment`；
2. 设置 `kMMExePath` 与 `user-lib-dir`，注册 `kMMReadPush` 等回调；
3. `StartMMMojoEnvironment`，等子进程连上；
4. 发 `OcrRequest{task_id, pic_path}`；
5. 回调里收 `OcrResponse`，取 `single_result[].single_str_utf8` 拼成文本。

## 怎么用

设置 → 文字识别 → 引擎列表里选「微信 OCR（本地）」。这一行有「检测」按钮，
会显示本机是否能找到微信 OCR、路径是什么。之后横条 OCR、快捷键、网格页
都走它。

## 前提

- 安装微信 PC 版（3.9 / 4.x），并至少启动过一次；
- 微信的 OCR 模型在 `%APPDATA%\Tencent\WeChat\XPlugin\Plugins\WeChatOCR\`
  下，`mmmojo_64.dll` 在微信安装目录里。

## 实测

用参考项目自带的 `WeChatOCR.exe` + `mmmojo_64.dll` 和它的 `test_img/1.png`
跑通了完整链路：识别结果返回了图里的全部英文（GitLab Community Edition 那段），
说明 mmmojo 初始化、回调、protobuf 收发都是通的。

## 验证

- `cargo test` 498 passed / 2 ignored；clippy `-D warnings` 干净；
- `tools/check-all.js` 全过；
- protobuf 请求/响应编解码单测通过。
