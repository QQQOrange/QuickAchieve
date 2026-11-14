# Quick Achieve v2.23.126

> 修微信 4.1+ 上「微信 OCR」用不了。

## 原因

微信 4.1 起把 OCR 收进了 `WeChatOcr.bin` 插件（zip 内含 `wxocr.dll` 和模型），
不再往 `%APPDATA%\Tencent\WeChat\XPlugin\Plugins\WeChatOCR\...` 释放独立的
`WeChatOCR.exe`。原来的探测只看这个 exe，所以 4.1 上会报“没找到”。

## 修复

`wechat_ocr::detect()` 增加数据目录兼容引擎：

```
<数据目录>\wechat-ocr\WeChatOCR.exe
<数据目录>\wechat-ocr\mmmojo_64.dll
<数据目录>\wechat-ocr\Model\...
```

微信 3.9 的独立引擎可以脱离微信主程序运行；放在这里后，4.1+ 也能直接用。
查找顺序：环境变量 → 数据目录兼容引擎 → 注册表 / XPlugin 自动探测。

## 使用

把 3.9 的 `WeChatOCR.exe`、`mmmojo_64.dll` 和 `Model` 目录放到
`%APPDATA%\QuickAchieve\wechat-ocr\`（便携模式则在
`Quick Achieve data\wechat-ocr\`），然后在设置里选「微信 OCR（本地）」，
点「检测」应能看到这个路径。

## 验证

- `cargo test` 498 passed / 2 ignored；clippy 干净；
- 用 3.9 引擎 + `test_img/1.png` 实测，识别结果与图内文字一致。
